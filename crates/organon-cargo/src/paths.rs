//! Rewrite generated absolute Rust crate paths without modifying literal data.
use quote::ToTokens;

/// Replace one generated crate root with the facade's path, including qualified
/// associated items and imports. Literal text and relative paths are untouched.
/// Macro arguments are opaque: generators must emit paths to rewrite as Rust
/// syntax outside macro invocations.
pub fn replace_crate_path(
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
