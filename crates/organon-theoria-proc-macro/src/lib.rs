//! Import an Organon XML definition as Rust items, without a build script.
//!
//! This is the implementation crate; consumers use the re-export in `organon-theoria`.
//!
//! `library!(crate::module)` loads the crate-root `organon.xml`. Public Rust items are generated
//! in the invocation module; the exported `organon!` macro connects providers
//! without a crate-root module re-export. `implement` supplies Rust behavior for declarations.

use proc_macro::TokenStream;
use quote::{ToTokens, quote};
use std::path::PathBuf;
use syn::{
    Token,
    parse::{Parse, ParseStream},
    parse_macro_input,
};

// Keep the facade's hygienic $crate token instead of guessing a Cargo alias.
// Only generated absolute runtime paths change; literals and other names remain intact.
fn runtime_paths(
    code: proc_macro2::TokenStream,
    runtime: &syn::Path,
) -> syn::Result<proc_macro2::TokenStream> {
    replace_paths(code, "organon_theoria", runtime)
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

struct Library {
    runtime: syn::Path,
    location: syn::Path,
}
impl Parse for Library {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let runtime = input.parse()?;
        input.parse::<Token![;]>()?;
        if input.is_empty() {
            return Err(input.error(
                "library! requires its containing module path, for example crate::graphics",
            ));
        }
        let location: syn::Path = input.parse()?;
        Ok(Self { runtime, location })
    }
}

mod native;

#[proc_macro]
pub fn library(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as Library);
    let path = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("organon.xml");
    let result = (|| -> syn::Result<proc_macro2::TokenStream> {
        let location = &input.location;
        let connector = organon_cargo::export_connector(location)?;
        let code = organon_theoria_compiler::library::compile(
            &path,
            &location.to_token_stream().to_string(),
        )
        .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), e.to_string()))?;
        let code = runtime_paths(code.parse()?, &input.runtime)?;
        Ok(quote! { #connector #code })
    })();
    result.unwrap_or_else(syn::Error::into_compile_error).into()
}
#[proc_macro_attribute]
pub fn implement(attribute: TokenStream, item: TokenStream) -> TokenStream {
    if !attribute.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "implement takes no arguments; use the declaration name as the impl target",
        )
        .into_compile_error()
        .into();
    }
    let item = match syn::parse::<syn::Item>(item) {
        Ok(syn::Item::Impl(item)) => item,
        _ => {
            return syn::Error::new(
                proc_macro2::Span::call_site(),
                "implement belongs on an impl block",
            )
            .into_compile_error()
            .into();
        }
    };
    native::implement(item)
        .and_then(|code| {
            let linked = organon_cargo::connect_library_paths(&code.to_string(), None)
                .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), e.to_string()))?;
            let linked: proc_macro2::TokenStream = linked.parse()?;
            Ok(quote!(const _: () = { #linked };))
        })
        .and_then(native::runtime)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
