//! Compile native adapters for the Organon application's IR evaluator.
//! This macro is an application tool, not part of the normal Organon compilation path.
use proc_macro::TokenStream;
use quote::ToTokens;
use std::path::PathBuf;
use syn::{
    LitStr, Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

mod generate;

struct Preview {
    host: syn::Path,
    path: LitStr,
}
impl Parse for Preview {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let host = input.parse()?;
        input.parse::<Token![;]>()?;
        Ok(Self {
            host,
            path: input.parse()?,
        })
    }
}

/// Generate adapters only in the preview host. Providers need no preview registration.
#[proc_macro]
pub fn preview(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Preview);
    let result = (|| -> syn::Result<proc_macro2::TokenStream> {
        let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
        let code = generate::compile(&root.join(input.path.value()), &root.join("Cargo.toml"))
            .map_err(|e| syn::Error::new(input.path.span(), e.to_string()))?;
        let host = &input.host;
        let runtime = syn::parse_quote!(#host::runtime);
        let code = replace_paths(code.parse()?, "organon_theoria", &runtime)?;
        replace_paths(code, "organon_preview", host)
    })();
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}

fn replace_paths(
    code: proc_macro2::TokenStream,
    name: &str,
    runtime: &syn::Path,
) -> syn::Result<proc_macro2::TokenStream> {
    use syn::visit_mut::VisitMut;
    struct RuntimePaths<'a>(&'a syn::Path, &'a str);
    impl VisitMut for RuntimePaths<'_> {
        fn visit_type_path_mut(&mut self, ty: &mut syn::TypePath) {
            let replaced = ty.path.leading_colon.is_some()
                && ty.path.segments.first().is_some_and(|s| s.ident == self.1);
            syn::visit_mut::visit_type_path_mut(self, ty);
            if replaced
                && let Some(qself) = &mut ty.qself
                && qself.position > 0
            {
                qself.position += self.0.segments.len() - 1;
            }
        }

        fn visit_expr_path_mut(&mut self, expr: &mut syn::ExprPath) {
            let replaced = expr.path.leading_colon.is_some()
                && expr
                    .path
                    .segments
                    .first()
                    .is_some_and(|s| s.ident == self.1);
            syn::visit_mut::visit_expr_path_mut(self, expr);
            if replaced
                && let Some(qself) = &mut expr.qself
                && qself.position > 0
            {
                qself.position += self.0.segments.len() - 1;
            }
        }

        fn visit_item_use_mut(&mut self, item: &mut syn::ItemUse) {
            if item.leading_colon.is_some()
                && let syn::UseTree::Path(tree) = &item.tree
                && tree.ident == self.1
            {
                let runtime = self.0;
                let tail = &tree.tree;
                let replacement: syn::ItemUse = syn::parse_quote!(use #runtime::#tail;);
                item.leading_colon = replacement.leading_colon;
                item.tree = replacement.tree;
            }
            syn::visit_mut::visit_item_use_mut(self, item);
        }
        fn visit_path_mut(&mut self, path: &mut syn::Path) {
            syn::visit_mut::visit_path_mut(self, path);
            if path.leading_colon.is_some()
                && path
                    .segments
                    .first()
                    .is_some_and(|part| part.ident == self.1)
            {
                let tail: Vec<_> = path.segments.iter().skip(1).cloned().collect();
                *path = self.0.clone();
                path.segments.extend(tail);
            }
        }
    }
    let mut file: syn::File = syn::parse2(code)?;
    RuntimePaths(runtime, name).visit_file_mut(&mut file);
    Ok(file.into_token_stream())
}
