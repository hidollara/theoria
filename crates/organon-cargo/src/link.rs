//! Resolve generated contract paths through each provider's exported macro.
//!
//! Provider module locations are owned by their `organon!` macro. Connections
//! are local to each generated Rust module, including references inside macros.
use proc_macro2::{Group, Ident, TokenStream, TokenTree};
use quote::{ToTokens, format_ident, quote};
use std::collections::BTreeMap;

pub fn connect(code: &str, location: Option<&str>) -> Result<String, crate::Error> {
    let mut file: syn::File = syn::parse_str(code).map_err(|e| crate::Error(e.to_string()))?;
    let location = location
        .map(syn::parse_str::<syn::Path>)
        .transpose()
        .map_err(|e| crate::Error(e.to_string()))?;
    scope(&mut file.items, location.as_ref())?;
    Ok(file.into_token_stream().to_string())
}

fn scope(items: &mut Vec<syn::Item>, location: Option<&syn::Path>) -> Result<(), crate::Error> {
    let mut links = Links {
        location,
        aliases: BTreeMap::new(),
        declarations: Vec::new(),
    };
    for item in items.iter_mut() {
        if let syn::Item::Mod(module) = item
            && let Some((_, items)) = &mut module.content
        {
            scope(items, location)?;
        } else {
            *item = syn::parse2(links.tokens(item.to_token_stream()))
                .map_err(|e| crate::Error(e.to_string()))?;
        }
    }
    let declarations = links.declarations;
    let mut imports: syn::File =
        syn::parse2(quote!(#(#declarations)*)).map_err(|e| crate::Error(e.to_string()))?;
    imports.items.append(items);
    *items = imports.items;
    Ok(())
}

struct Links<'a> {
    location: Option<&'a syn::Path>,
    aliases: BTreeMap<String, Ident>,
    declarations: Vec<TokenStream>,
}

impl Links<'_> {
    fn path(&mut self, mut path: syn::Path) -> syn::Path {
        while let Some(index) = path.segments.iter().position(|s| s.ident == "__organon") {
            if index == 0 {
                break;
            }
            let mut provider = path.clone();
            provider.segments = path.segments.iter().take(index).cloned().collect();
            let tail: Vec<_> = path.segments.iter().skip(index + 1).cloned().collect();
            if provider.is_ident("crate")
                && let Some(location) = self.location
            {
                let mut resolved = location.clone();
                resolved.segments.push(syn::parse_quote!(__organon));
                // Do not resolve this local contract again. Dependencies after it
                // still need a connection to their own provider macro.
                if let Some(next) = tail.iter().position(|s| s.ident == "__organon") {
                    resolved.segments.extend(tail.iter().take(next).cloned());
                    let alias = self.alias(&resolved);
                    path = syn::parse_quote!(#alias::library);
                    path.segments.extend(tail.into_iter().skip(next + 1));
                    continue;
                }
                resolved.segments.extend(tail);
                return resolved;
            }
            let alias = self.alias(&provider);
            path = syn::parse_quote!(#alias::library);
            path.segments.extend(tail);
        }
        path
    }

    fn alias(&mut self, provider: &syn::Path) -> Ident {
        let key = provider.to_token_stream().to_string();
        if let Some(alias) = self.aliases.get(&key) {
            return alias.clone();
        }
        let alias = format_ident!("__organon_link_{}", self.aliases.len());
        let mut import = provider.clone();
        if import.leading_colon.is_none()
            && import
                .segments
                .first()
                .is_some_and(|s| s.ident.to_string().starts_with("__organon_link_"))
        {
            import.segments.insert(0, syn::parse_quote!(super));
        }
        self.aliases.insert(key, alias.clone());
        let connect = if provider.is_ident("crate") {
            quote! {
                #[allow(unused_imports)]
                use crate::*;
                use organon as __connect;
            }
        } else {
            quote!(use #import::organon as __connect;)
        };
        self.declarations.push(quote! {
            #[doc(hidden)]
            mod #alias {
                // A glob also supports the current crate's macro-generated
                // export without the absolute-macro-path lint.
                #connect
                __connect!(@link library);
            }
        });
        alias
    }

    fn tokens(&mut self, stream: TokenStream) -> TokenStream {
        let tokens: Vec<_> = stream.into_iter().collect();
        let mut result = TokenStream::new();
        let mut index = 0;
        while index < tokens.len() {
            if let TokenTree::Group(group) = &tokens[index] {
                let mut replacement = Group::new(group.delimiter(), self.tokens(group.stream()));
                replacement.set_span(group.span());
                result.extend([TokenTree::Group(replacement)]);
                index += 1;
                continue;
            }
            let start = index;
            let mut end = index;
            if colon_pair(&tokens, end) {
                end += 2;
            }
            if matches!(tokens.get(end), Some(TokenTree::Ident(_))) {
                end += 1;
                while colon_pair(&tokens, end)
                    && matches!(tokens.get(end + 2), Some(TokenTree::Ident(_)))
                {
                    end += 3;
                }
                let candidate: TokenStream = tokens[start..end].iter().cloned().collect();
                if let Ok(path) = syn::parse2::<syn::Path>(candidate)
                    && path.segments.iter().any(|s| s.ident == "__organon")
                {
                    result.extend(self.path(path).into_token_stream());
                    index = end;
                    continue;
                }
            }
            result.extend([tokens[index].clone()]);
            index += 1;
        }
        result
    }
}

fn colon_pair(tokens: &[TokenTree], index: usize) -> bool {
    matches!(tokens.get(index), Some(TokenTree::Punct(p)) if p.as_char() == ':')
        && matches!(tokens.get(index + 1), Some(TokenTree::Punct(p)) if p.as_char() == ':')
}

/// Export the provider connector and verify the explicitly supplied module location.
pub fn export_connector(location: &syn::Path) -> syn::Result<TokenStream> {
    use syn::ext::IdentExt;
    if location.leading_colon.is_some()
        || location.segments.first().is_none_or(|s| s.ident != "crate")
        || location.segments.iter().any(|s| {
            !matches!(s.arguments, syn::PathArguments::None)
                || s.ident == "self"
                || s.ident == "super"
        })
    {
        return Err(syn::Error::new_spanned(
            location,
            "library! requires a crate-rooted module path",
        ));
    }
    let tail: Vec<_> = location.segments.iter().skip(1).map(|s| &s.ident).collect();
    let suffix = tail
        .iter()
        .map(|s| format!("::{}", s.unraw()))
        .collect::<String>();
    // Preserve the caller's crate identity, not the facade macro's `$crate`.
    let connector = quote::quote_spanned! {location.segments[0].ident.span()=>
        #[doc(hidden)]
        #[macro_export]
        macro_rules! organon {
            (@link $alias:ident) => {
                pub use $crate #(::#tail)* ::__organon as $alias;
            };
        }
    };
    Ok(quote! {
        const _: () = {
            let actual = module_path!().as_bytes();
            let mut root = 0;
            while root < actual.len() && actual[root] != b':' { root += 1; }
            let expected = #suffix.as_bytes();
            assert!(actual.len() - root == expected.len(), "library! path must match its containing module");
            let mut index = 0;
            while index < expected.len() {
                assert!(actual[root + index] == expected[index], "library! path must match its containing module");
                index += 1;
            }
        };
        #connector
    })
}

/// Register files read during macro expansion as rustc inputs.
pub fn track_files<P: AsRef<std::path::Path>>(
    paths: impl IntoIterator<Item = P>,
) -> Result<TokenStream, crate::Error> {
    let paths = paths
        .into_iter()
        .map(|path| {
            path.as_ref()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| crate::Error("tracked source path must be valid UTF-8".into()))
        })
        .collect::<Result<std::collections::BTreeSet<_>, _>>()?;
    Ok(quote! { #(const _: &str = ::core::include_str!(#paths);)* })
}
