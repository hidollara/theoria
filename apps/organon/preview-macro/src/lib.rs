//! Compile native adapters for the Organon application's IR evaluator.
//! This macro is an application tool, not part of the normal Organon compilation path.
use proc_macro::TokenStream;
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
        let code = organon_cargo::replace_crate_path(code.parse()?, "organon_theoria", &runtime)?;
        organon_cargo::replace_crate_path(code, "organon_preview", host)
    })();
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}
