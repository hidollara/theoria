//! Prepare attribute syntax and reference requirements in const code.
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, Type};

/// A third ParseBuffer argument opts into storage supplied by the caller.
pub(super) fn buffered(signature: &syn::Signature) -> syn::Result<bool> {
    if signature.constness.is_none() {
        return Err(syn::Error::new_spanned(
            signature,
            "#[parse] requires a const fn; construct runtime values in the returned closure",
        ));
    }
    let constants: Vec<_> = signature.generics.const_params().collect();
    if signature.inputs.len() == 2 && constants.is_empty() {
        return Ok(false);
    }
    let valid_buffer = signature.inputs.last().is_some_and(|arg| {
        matches!(arg, syn::FnArg::Typed(arg) if matches!(&*arg.ty,
            Type::Path(path) if path.path.segments.last().is_some_and(|s| s.ident == "ParseBuffer")))
    });
    if signature.constness.is_some()
        && signature.inputs.len() == 3
        && valid_buffer
        && constants.len() == 1
        && matches!(&constants[0].ty, Type::Path(path) if path.path.is_ident("usize"))
        && signature.generics.type_params().next().is_none()
    {
        return Ok(true);
    }
    Err(syn::Error::new_spanned(
        signature,
        "expected parse(text, context), or const parse<const N: usize>(text, context, buffer: ParseBuffer<..., N>)",
    ))
}

pub(super) fn expand(
    target: &Type,
    marker: &Ident,
    function: &Ident,
    buffered: bool,
) -> TokenStream {
    let parser = format_ident!("__OrganonParser_{marker}");
    let function = quote!(<#target>::#function);
    let parse = if buffered {
        quote!(#function::<N>(text, &mut cx, ::organon_theoria::attribute::ParseBuffer::array()))
    } else {
        quote!(#function(text, &mut cx))
    };
    let measure = if buffered {
        quote! {
            let mut count = 0;
            let parsed = #function::<0>(text, &mut cx,
                ::organon_theoria::attribute::ParseBuffer::counting(&mut count));
            let error = match &parsed { Ok(_) => None, Err(error) => Some(*error) };
            // Never run the construction closure during measurement. Moving the
            // whole Result into forget also avoids requiring const Drop or Copy.
            ::std::mem::forget(parsed);
            match error { Some(error) => Err(error), None => Ok(count) }
        }
    } else {
        quote! { let _ = (text, cx); Ok(0) }
    };
    let dynamic_parse = if buffered {
        quote! {
            let size = #parser.measure(text, ::organon_theoria::attribute::ParseContext::new(&schema)).map_err(|error| error.to_string())?;
            let mut storage = vec![None; size];
            let build = #function::<0>(text, &mut cx,
                ::organon_theoria::attribute::ParseBuffer::slice(&mut storage))
                .map_err(|error| error.to_string())?;
        }
    } else {
        quote! { let build = #function(text, &mut cx).map_err(|error| error.to_string())?; }
    };
    let requirements = if buffered {
        quote! {
            let mut count = 0;
            let parsed = #function::<0>(text, &mut cx, ::organon_theoria::attribute::ParseBuffer::counting(&mut count));
            if let Err(error) = &parsed { panic!("{}", error.message()); }
            ::std::mem::forget(parsed);
        }
    } else {
        quote! {
            let parsed = #function(text, &mut cx);
            if let Err(error) = &parsed { panic!("{}", error.message()); }
            ::std::mem::forget(parsed);
        }
    };
    let prepare = {
        quote! {
            let parsed = #parse;
            if let Err(error) = &parsed {
                panic!("{}", error.message());
            }
            // Capture the entire Result. Moving an opaque builder out here would
            // require const Drop support; moving it into a closure does not.
            move |scope| {
                let build = parsed.map_err(|error| error.to_string())?;
                let mut resolve = ::organon_theoria::attribute::ResolveContext::new(scope);
                let source = build(&mut resolve)?;
                Ok(resolve.finish(source))
            }
        }
    };
    quote! {
        #[doc(hidden)]
        #[allow(non_camel_case_types)]
        pub struct #parser;
        impl #parser {
            pub const fn dependencies<const N: usize>(
                &self, text: &str,
                bindings: &[::organon_theoria::attribute::BindingType<'_>],
            ) -> [bool; N] {
                let mut references = [false; N];
                let mut cx = ::organon_theoria::attribute::ParseContext::recording(bindings, &mut references);
                #requirements
                references
            }

            pub const fn measure(
                &self,
                text: &str,
                mut cx: ::organon_theoria::attribute::ParseContext<'_>,
            ) -> Result<usize, ::organon_theoria::attribute::ParseError> {
                #measure
            }

            pub const fn prepare<'a, const N: usize>(
                &self,
                text: &'a str,
                mut cx: ::organon_theoria::attribute::ParseContext<'a>,
            ) -> impl FnOnce(&::organon_theoria::attribute::Scope)
                -> Result<::organon_theoria::attribute::Attribute<<#target as ::organon_theoria::Type>::Value>, String> + use<'a, N> {
                #prepare
            }

        }
        impl #target {
            pub const __ORGANON_PARSER: #parser = #parser;

            pub fn __organon_parse(text: &str, scope: &::organon_theoria::attribute::Scope)
                -> Result<::organon_theoria::attribute::Attribute<<Self as ::organon_theoria::Type>::Value>, String> {
                let schema = scope.schema();
                let mut cx = ::organon_theoria::attribute::ParseContext::new(&schema);
                #dynamic_parse
                let mut resolve = ::organon_theoria::attribute::ResolveContext::new(scope);
                let source = build(&mut resolve)?;
                Ok(resolve.finish(source))
            }
        }
    }
}
