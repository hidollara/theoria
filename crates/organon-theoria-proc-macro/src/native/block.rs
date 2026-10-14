//! One implementation block owns a declaration's native representation and outputs.
use super::*;
use syn::visit_mut::VisitMut;

pub(super) fn expand(binding: Binding, item: syn::ItemImpl) -> syn::Result<TokenStream> {
    if item.trait_.is_some() || !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item,
            "expected a nongeneric inherent implementation",
        ));
    }
    let marker = marker(&binding)?;
    let target: syn::Type = syn::parse_quote!(crate::__organon::#marker);
    let mut result = TokenStream::new();
    let mut methods = Vec::new();
    let mut value = None;
    let mut parser = None;
    let mut outputs = Vec::new();
    let mut forwards = Vec::new();
    for member in item.items {
        match member {
            syn::ImplItem::Type(v) if v.ident == "Value" => value = Some(v.ty),
            syn::ImplItem::Fn(mut f) => {
                let mut bindings = Vec::new();
                let mut parse_attributes = Vec::new();
                f.attrs.retain(|a| {
                    if a.path().is_ident("parse") {
                        parse_attributes.push(a.clone());
                        return false;
                    }
                    if a.path().is_ident("output") {
                        bindings.push(a.clone());
                        false
                    } else {
                        true
                    }
                });
                if parse_attributes.len() > 1 || (!parse_attributes.is_empty() && parser.is_some())
                {
                    return Err(syn::Error::new_spanned(
                        &f.sig,
                        "only one #[parse] implementation is allowed",
                    ));
                }
                if let Some(attribute) = parse_attributes.first() {
                    if !matches!(attribute.meta, syn::Meta::Path(_)) {
                        return Err(syn::Error::new_spanned(
                            attribute,
                            "#[parse] takes no arguments",
                        ));
                    }
                    if !bindings.is_empty() {
                        return Err(syn::Error::new_spanned(
                            attribute,
                            "a parser cannot also be an output",
                        ));
                    }
                }
                let mut signature = f.sig.clone();
                let ident = &f.sig.ident;
                let helper = format_ident!("__organon_{}_{}", marker, ident);
                signature.ident = helper.clone();
                Rewrite {
                    target: target.clone(),
                }
                .visit_signature_mut(&mut signature);
                Rewrite {
                    target: target.clone(),
                }
                .visit_block_mut(&mut f.block);
                let body = &f.block;
                let function = quote!(#[allow(non_snake_case)] #signature #body);
                if !parse_attributes.is_empty() {
                    parser = Some(helper);
                    result.extend(function);
                } else if !bindings.is_empty() {
                    for attribute in bindings {
                        let mut b = binding.clone();
                        attribute.parse_nested_meta(|meta| {
                            if meta.path.is_ident("name") {
                                b.output = Some(meta.value()?.parse()?);
                            } else if meta.path.is_ident("interface") {
                                b.interface = Some(meta.value()?.parse()?);
                            } else {
                                return Err(meta.error("expected name or interface"));
                            }
                            Ok(())
                        })?;
                        outputs.push((b, function.clone()));
                    }
                } else {
                    Rewrite {
                        target: target.clone(),
                    }
                    .visit_signature_mut(&mut f.sig);
                    methods.push(f);
                }
            }
            syn::ImplItem::Macro(m) if m.mac.path.is_ident("forward") => {
                struct Forward {
                    input: Ident,
                    output: Ident,
                }
                impl Parse for Forward {
                    fn parse(p: ParseStream<'_>) -> syn::Result<Self> {
                        let input = p.parse()?;
                        p.parse::<Token![=>]>()?;
                        Ok(Self {
                            input,
                            output: p.parse()?,
                        })
                    }
                }
                let f: Forward = syn::parse2(m.mac.tokens)?;
                forwards.push((f.input, f.output));
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "expected type Value, a method, or forward!(input => output)",
                ));
            }
        }
    }
    if let Some(value) = value {
        let b = binding.clone();
        let alias = format_ident!("__OrganonValue_{}", marker);
        result.extend(implement_value(
            b,
            quote!(#[allow(non_camel_case_types)] type #alias = #value;),
        )?);
        if let Some(parser) = parser {
            // Inherent dispatch lets the Type implementation remain uniform.
            result.extend(quote! {
                impl #target {
                    pub const __ORGANON_REFERENCE_ONLY: bool = false;
                    pub fn __organon_parse(text: &str, scope: &::organon_theoria::attribute::Scope)
                        -> Result<::organon_theoria::attribute::Attribute<<Self as ::organon_theoria::Type>::Value>, String> {
                        let mut context = ::organon_theoria::attribute::ParseContext::new(scope);
                        let source = #parser(text, &mut context)?;
                        Ok(context.finish(source))
                    }
                }
            });
        }
    } else if let Some(parser) = parser {
        let loaded = &binding.library;
        let interface=loaded.loaded.file.definitions().any(|d| matches!(d,organon_core::format::Definition::Interface(i) if i.name==binding.name.value()));
        let method = if interface {
            format_ident!("__organon_interpret")
        } else {
            format_ident!("__organon_parse")
        };
        result.extend(quote! {
            impl #target {
                pub const __ORGANON_REFERENCE_ONLY: bool = false;
                pub fn #method(text: &str, scope: &::organon_theoria::attribute::Scope)
                    -> Result<::organon_theoria::attribute::Attribute<<Self as ::organon_theoria::Type>::Value>, String> {
                    let mut context = ::organon_theoria::attribute::ParseContext::new(scope);
                    let source = #parser(text, &mut context)?;
                    Ok(context.finish(source))
                }
            }
        });
    }
    let mut emitted = std::collections::BTreeSet::new();
    for (b, function) in outputs {
        let expanded = implement_output(b, function)?;
        let mut file: syn::File = syn::parse2(expanded)?;
        file.items.retain(|i| match i {
            syn::Item::Fn(f) => emitted.insert(f.sig.ident.to_string()),
            _ => true,
        });
        result.extend(quote!(#file));
    }
    for (input, output) in forwards {
        let output_marker = format_ident!("{marker}_{output}");
        let function = format_ident!("__organon_forward_{marker}_{output}");
        let mut b = binding.clone();
        b.output = Some(LitStr::new(&output.to_string(), output.span()));
        let expanded = implement_output(
            b,
            quote! {
                #[allow(non_snake_case)]
                fn #function(input: crate::__organon::ReadInput<'_, '_, '_, #target>)
                    -> <crate::__organon::#output_marker as ::organon_theoria::Output>::Value {
                    input.#input().into_cloned()
                }
            },
        )?;
        let mut file: syn::File = syn::parse2(expanded)?;
        for item in &mut file.items {
            if let syn::Item::Impl(i) = item {
                i.items.push(syn::parse_quote! {
                    fn calculation(input: std::rc::Rc<Self::Input>) -> ::organon_theoria::Calculation<Self::Value> {
                        ::organon_theoria::Calculation::forward(input.#input().clone())
                    }
                });
            }
        }
        result.extend(quote!(#file));
    }
    result.extend(quote!(impl #target { #(#methods)* }));
    Ok(result)
}

struct Rewrite {
    target: syn::Type,
}
impl VisitMut for Rewrite {
    fn visit_expr_path_mut(&mut self, expression: &mut syn::ExprPath) {
        if expression.qself.is_none()
            && expression
                .path
                .segments
                .first()
                .is_some_and(|p| p.ident == "Self")
            && let syn::Type::Path(target) = &self.target
        {
            let tail: Vec<_> = expression.path.segments.iter().skip(1).cloned().collect();
            expression.path = target.path.clone();
            expression.path.segments.extend(tail);
        }
        syn::visit_mut::visit_expr_path_mut(self, expression);
    }
    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if let syn::Type::Path(p) = ty {
            if p.path.is_ident("Self") {
                *ty = self.target.clone();
                return;
            }
            if p.path.segments.len() == 2
                && p.path.segments[0].ident == "Self"
                && p.path.segments[1].ident == "Value"
            {
                let target = &self.target;
                *ty = syn::parse_quote!(<#target as ::organon_theoria::Type>::Value);
                return;
            }
            if p.path.segments.last().is_some_and(|s| s.ident == "Input") {
                let target = &self.target;
                *ty = syn::parse_quote!(crate::__organon::ReadInput<'_, '_, '_, #target>);
                return;
            }
        }
        syn::visit_mut::visit_type_mut(self, ty);
    }
}
