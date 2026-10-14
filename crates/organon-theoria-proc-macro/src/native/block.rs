//! One implementation block owns a declaration's native representation and outputs.
use super::*;
use syn::visit_mut::VisitMut;

pub(super) fn expand(binding: Binding, item: syn::ItemImpl) -> syn::Result<TokenStream> {
    if item.trait_.is_some()
        || !item.generics.params.is_empty()
        || item.generics.where_clause.is_some()
    {
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
    let mut forwards = Vec::new();
    for member in item.items {
        match member {
            syn::ImplItem::Type(v) if v.ident == "Value" => {
                if value.is_some() {
                    return Err(syn::Error::new_spanned(v, "only one type Value is allowed"));
                }
                value = Some(v.ty);
            }
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
                let ident = f.sig.ident.clone();
                Rewrite {
                    target: target.clone(),
                }
                .visit_impl_item_fn_mut(&mut f);
                if !parse_attributes.is_empty() {
                    let buffered = super::parser::buffered(&f.sig)?;
                    parser = Some((ident.clone(), buffered));
                } else if !bindings.is_empty() {
                    let computation = (bindings.len() > 1).then(|| {
                        let name = format_ident!("__OrganonComputation_{}_{}", marker, ident);
                        result.extend(quote!(#[allow(non_camel_case_types)] struct #name;));
                        name
                    });
                    for attribute in bindings {
                        let mut b = binding.clone();
                        b.computation = computation.clone();
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
                        result.extend(implement_output(b, &mut f)?);
                    }
                }
                methods.push(f);
            }
            syn::ImplItem::Macro(m) if m.mac.path.is_ident("forward") => {
                struct Forward {
                    input: Ident,
                    interface: Option<Ident>,
                    output: Ident,
                }
                impl Parse for Forward {
                    fn parse(p: ParseStream<'_>) -> syn::Result<Self> {
                        let input = p.parse()?;
                        p.parse::<Token![=>]>()?;
                        let target = p.parse()?;
                        let (interface, output) = if p.peek(Token![.]) {
                            p.parse::<Token![.]>()?;
                            (Some(target), p.parse()?)
                        } else {
                            (None, target)
                        };
                        Ok(Self {
                            input,
                            interface,
                            output,
                        })
                    }
                }
                let f: Forward = syn::parse2(m.mac.tokens)?;
                forwards.push((f.input, f.interface, f.output));
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
        result.extend(implement_value(&binding, &value)?);
    }
    if let Some((function, buffered)) = parser {
        result.extend(super::parser::expand(&target, &marker, &function, buffered));
    }
    for (input, interface, output) in forwards {
        let mut b = binding.clone();
        b.output = Some(LitStr::new(&output.to_string(), output.span()));
        b.interface = interface
            .as_ref()
            .map(|i| LitStr::new(&i.to_string(), i.span()));
        let field = if let Some(interface) = interface {
            let declaration = binding
                .library
                .program
                .contracts
                .get(&binding.name.value())
                .filter(|c| matches!(c.kind, DefinitionKind::DeclareElement { .. }))
                .ok_or_else(|| syn::Error::new(output.span(), "not an Element declaration"))?;
            organon_theoria_compiler::library::interface_output_name(
                declaration,
                &interface.to_string(),
                &output.to_string(),
            )
        } else {
            output.to_string()
        };
        let output_marker = format_ident!("{marker}_{field}");
        let function = format_ident!("__organon_forward_{marker}_{field}");
        let mut method: syn::ImplItemFn = syn::parse_quote! {
                #[allow(non_snake_case)]
                fn #function(input: crate::__organon::ReadInput<'_, '_, '_, #target>)
                    -> <crate::__organon::#output_marker as ::organon_theoria::Output>::Value {
                    input.#input().into_cloned()
                }
        };
        let mut implementation: syn::ItemImpl = syn::parse2(implement_output(b, &mut method)?)?;
        implementation.items.push(syn::parse_quote! {
            fn calculation(input: std::rc::Rc<Self::Input>) -> ::organon_theoria::Calculation<Self::Value> {
                ::organon_theoria::Calculation::forward(input.#input().clone())
            }
        });
        methods.push(method);
        result.extend(quote!(#implementation));
    }
    result.extend(quote!(impl #target { #(#methods)* }));
    Ok(result)
}

struct Rewrite {
    target: syn::Type,
}
impl VisitMut for Rewrite {
    // Nested items have their own Self and generic scope.
    fn visit_item_mut(&mut self, _item: &mut syn::Item) {}

    fn visit_type_mut(&mut self, ty: &mut syn::Type) {
        if let syn::Type::Path(p) = ty {
            if p.path.segments.len() == 2
                && p.path.segments[0].ident == "Self"
                && p.path.segments[1].ident == "Value"
            {
                let target = &self.target;
                *ty = syn::parse_quote!(<#target as ::organon_theoria::Type>::Value);
                return;
            }
            if p.qself.is_none()
                && p.path.segments.len() == 1
                && p.path.segments[0].ident == "Input"
                && let syn::PathArguments::AngleBracketed(args) = &p.path.segments[0].arguments
                && args.args.len() == 2
                && matches!(args.args.first(), Some(syn::GenericArgument::Lifetime(_)))
                && matches!(args.args.last(), Some(syn::GenericArgument::Type(syn::Type::Path(t))) if t.qself.is_none() && t.path.is_ident("Self"))
            {
                let target = &self.target;
                *ty = syn::parse_quote!(crate::__organon::ReadInput<'_, '_, '_, #target>);
                return;
            }
        }
        syn::visit_mut::visit_type_mut(self, ty);
    }
}
