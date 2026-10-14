//! Import an Organon XML definition as Rust items, without a build script.
//!
//! This is the implementation crate; consumers use the re-export in `organon`.
//!
//! Place `organon::import!("src/panel.organon.xml");` inside a Rust
//! module to choose its namespace. Paths are relative to the consuming package's
//! Cargo.toml. Generated code depends directly on Theoria, not the Organon app.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use std::{collections::BTreeMap, path::PathBuf};
use syn::{
    LitStr, Token,
    ext::IdentExt,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

struct Import {
    path: LitStr,
    implementations: BTreeMap<String, String>,
}
impl Parse for Import {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let path = input.parse()?;
        let mut implementations = BTreeMap::new();
        if !input.is_empty() {
            input.parse::<Token![,]>()?;
            let keyword: syn::Ident = input.parse()?;
            if keyword != "implementations" {
                return Err(syn::Error::new(keyword.span(), "expected implementations"));
            }
            let content;
            syn::braced!(content in input);
            while !content.is_empty() {
                let name: syn::Ident = content.parse()?;
                content.parse::<Token![=]>()?;
                let expression: syn::Expr = content.parse()?;
                if implementations
                    .insert(
                        name.unraw().to_string(),
                        expression.into_token_stream().to_string(),
                    )
                    .is_some()
                {
                    return Err(syn::Error::new(
                        name.span(),
                        "duplicate implementation binding",
                    ));
                }
                if content.is_empty() {
                    break;
                }
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            path,
            implementations,
        })
    }
}

/// Compile one XML file's Element and interface declarations into the current module.
///
/// Each declaration's name determines its generated type name. Rust controls
/// module visibility. Changes to the XML file trigger recompilation.
/// XML imports are relative to their declaring file; all dependencies are tracked.
#[proc_macro]
pub fn import(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Import);
    expand(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(import: &Import) -> syn::Result<proc_macro2::TokenStream> {
    let input = &import.path;
    let error = |message| syn::Error::new(input.span(), message);
    let manifest = std::env::var_os("CARGO_MANIFEST_DIR")
        .ok_or_else(|| error("missing CARGO_MANIFEST_DIR".to_owned()))?;
    let path = PathBuf::from(manifest).join(input.value());
    let loaded =
        organon_compiler::load_definitions(&path).map_err(|cause| error(cause.to_string()))?;
    let generated = if import.implementations.is_empty() {
        organon_compiler::generate_loaded_file(&loaded.file)
    } else {
        organon_compiler::generate_file_with_implementations(&loaded.file, &import.implementations)
    };
    let code = generated.map_err(|cause| error(format!("{}: {cause}", path.display())))?;
    let code: proc_macro2::TokenStream = code.parse().map_err(|cause| {
        error(format!(
            "{}: invalid generated Rust: {cause}",
            path.display()
        ))
    })?;
    let paths = loaded
        .paths
        .iter()
        .map(|path| {
            path.to_str()
                .map(|path| LitStr::new(path, input.span()))
                .ok_or_else(|| error("XML path must be valid UTF-8".to_owned()))
        })
        .collect::<syn::Result<Vec<_>>>()?;
    // Reading a file in a proc macro alone does not register a rustc dependency.
    // Keep include_str! in the expansion so XML-only edits invalidate this crate.
    Ok(quote! {
        #(const _: &str = ::core::include_str!(#paths);)*
        #code
    })
}
