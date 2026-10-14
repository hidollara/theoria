use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::{path::PathBuf, rc::Rc};
use syn::{
    Ident, LitStr, Token,
    ext::IdentExt,
    parse::{Parse, ParseStream},
};
#[derive(Clone)]
struct Binding {
    library: Rc<organon_cargo::LoadedLibrary>,
    name: LitStr,
    output: Option<LitStr>,
    interface: Option<LitStr>,
    computation: Option<Ident>,
}

fn marker(binding: &Binding) -> syn::Result<Ident> {
    let origin = &binding.library.loaded.origins[&binding.name.value()];
    let name = organon_cargo::identity(&origin.source, &origin.name)
        .map_err(|e| syn::Error::new(binding.name.span(), e.to_string()))?;
    Ok(format_ident!("{name}"))
}

// In an implementation method the XML contract supplies the error in Result<T, _>.
fn infer_result_error(sig: &mut syn::Signature, contract: TokenStream) {
    let syn::ReturnType::Type(_, ty) = &mut sig.output else {
        return;
    };
    let syn::Type::Path(path) = ty.as_mut() else {
        return;
    };
    let Some(result) = path.path.segments.last_mut() else {
        return;
    };
    if result.ident != "Result" {
        return;
    }
    let syn::PathArguments::AngleBracketed(args) = &mut result.arguments else {
        return;
    };
    if args.args.len() == 2
        && matches!(
            args.args.last(),
            Some(syn::GenericArgument::Type(syn::Type::Infer(_)))
        )
    {
        *args.args.last_mut().unwrap() =
            syn::parse_quote!(<#contract as ::organon_theoria::ResultError>::Error);
    }
}

pub fn implement(mut item: syn::ItemImpl) -> syn::Result<TokenStream> {
    let syn::Type::Path(path) = item.self_ty.as_ref() else {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "expected a declaration name",
        ));
    };
    let ident = path
        .path
        .get_ident()
        .filter(|_| path.qself.is_none())
        .ok_or_else(|| {
            syn::Error::new_spanned(&item.self_ty, "expected an unqualified declaration name")
        })?;
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("organon.xml");
    let library = Rc::new(
        organon_cargo::load(&root)
            .map_err(|e| syn::Error::new_spanned(&item.self_ty, e.to_string()))?,
    );
    let requested = ident.unraw().to_string();
    let local = library
        .loaded
        .public
        .get(&requested)
        .cloned()
        .unwrap_or(requested);
    let definition = library
        .loaded
        .definitions
        .definitions()
        .find(|d| d.name() == local)
        .ok_or_else(|| syn::Error::new_spanned(&item.self_ty, "unknown declaration"))?;
    if matches!(definition, organon_core::syntax::Definition::Element(_)) {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "XML elements already define their implementation",
        ));
    }
    if matches!(
        definition,
        organon_core::syntax::Definition::DeclarePredicate(_)
    ) {
        let method = item
            .items
            .iter_mut()
            .find_map(|i| match i {
                syn::ImplItem::Fn(f) if f.sig.ident == "test" => Some(f),
                _ => None,
            })
            .ok_or_else(|| {
                syn::Error::new_spanned(&item.self_ty, "a native predicate requires fn test")
            })?;
        method.vis = syn::parse_quote!(pub(crate));
        let origin = &library.loaded.origins[&local];
        let id = organon_cargo::identity(&origin.source, &origin.name)
            .map_err(|e| syn::Error::new(ident.span(), e.to_string()))?;
        let result = format_ident!("__PredicateResult_{id}");
        infer_result_error(&mut method.sig, quote!(crate::__organon::#result));
    }
    let binding = Binding {
        library,
        name: LitStr::new(&local, ident.span()),
        output: None,
        interface: None,
        computation: None,
    };
    block::expand(binding, item)
}

fn implement_output(binding: Binding, function: &mut syn::ImplItemFn) -> syn::Result<TokenStream> {
    let name = function.sig.ident.clone();
    let marker = marker(&binding)?;
    let loaded = &binding.library;
    let file = &loaded.loaded.definitions;
    let declaration = file
        .definitions()
        .find_map(|d| match d {
            organon_core::syntax::Definition::DeclareElement(e)
                if e.name == binding.name.value() =>
            {
                Some(e)
            }
            _ => None,
        })
        .ok_or_else(|| syn::Error::new(binding.name.span(), "not an Element declaration"))?;
    let available: Vec<_> = if let Some(interface) = &binding.interface {
        let interface_name = interface.value();
        if !declaration.implements().any(|i| i == interface_name) {
            return Err(syn::Error::new(
                interface.span(),
                "Element does not implement this interface",
            ));
        }
        loaded.program.contracts[&interface_name]
            .outputs
            .iter()
            .map(|p| p.name.to_string())
            .collect()
    } else {
        declaration.outputs().map(|o| o.name.to_string()).collect()
    };
    let field = match &binding.output {
        Some(s) => s.value(),
        None if available.len() == 1 => available[0].clone(),
        None => {
            return Err(syn::Error::new(
                binding.name.span(),
                "select a named output",
            ));
        }
    };
    if !available.contains(&field) {
        return Err(syn::Error::new(binding.name.span(), "unknown output"));
    }
    let field = if let Some(interface) = &binding.interface {
        organon_theoria_compiler::library::interface_output_name(
            declaration,
            &interface.value(),
            &field,
        )
    } else {
        field
    };
    let output_marker = format_ident!("{marker}_{field}");
    infer_result_error(
        &mut function.sig,
        quote!(<crate::__organon::#output_marker as ::organon_theoria::Output>::Value),
    );
    let mut reads = vec![];
    let mut args = vec![];
    let mut native_value_argument = false;
    for argument in &function.sig.inputs {
        let syn::FnArg::Typed(argument) = argument else {
            return Err(syn::Error::new_spanned(
                argument,
                "expected a function argument",
            ));
        };
        let syn::Pat::Ident(pattern) = argument.pat.as_ref() else {
            return Err(syn::Error::new_spanned(
                &argument.pat,
                "input must have a name",
            ));
        };
        let ident = &pattern.ident;
        let spelling = ident.to_string();
        if spelling == "cx" {
            args.push(quote!(cx));
            continue;
        }
        if spelling == "input" && declaration.tag {
            args.push(quote!(crate::__organon::ReadInput::new(input, cx)));
            continue;
        }
        if !declaration.tag {
            if native_value_argument {
                return Err(syn::Error::new_spanned(
                    argument,
                    "native value output takes at most one value argument",
                ));
            }
            native_value_argument = true;
            args.push(quote!(input.clone()));
            continue;
        }
        let accessor = if spelling == "children" && declaration.children.is_some() {
            format_ident!("__children")
        } else if declaration.inputs().any(|p| p.name == spelling) {
            ident.clone()
        } else {
            return Err(syn::Error::new_spanned(
                ident,
                "argument does not name a declared input",
            ));
        };
        reads.push(quote!(let #ident=input.#accessor().read(cx);));
        args.push(quote!((*#ident).clone()));
    }
    let identity = binding.computation.as_ref().map(|id| {
        quote! {
            fn computation_id() -> Option<std::any::TypeId> {
                Some(std::any::TypeId::of::<#id>())
            }
        }
    });
    Ok(quote! {
      impl ::organon_theoria::Evaluate for crate::__organon::#output_marker {
       #identity
       fn evaluate(input:&Self::Input,cx:&::organon_theoria::ReadContext<'_, '_>)->Self::Value {
        let _=(input,cx); #(#reads)* crate::__organon::#marker::#name(#(#args),*)
       }
      }
    })
}

mod block;
mod parser;

fn implement_value(binding: &Binding, ty: &syn::Type) -> syn::Result<TokenStream> {
    let loaded = &binding.library;
    let declaration = loaded
        .loaded
        .definitions
        .definitions()
        .find_map(|d| match d {
            organon_core::syntax::Definition::DeclareElement(e)
                if !e.tag && e.name == binding.name.value() =>
            {
                Some(e)
            }
            _ => None,
        })
        .ok_or_else(|| syn::Error::new(binding.name.span(), "not a native value declaration"))?;
    let marker = marker(binding)?;
    let mut provisions = vec![];
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let paths = organon_cargo::CargoSources::default()
        .provider_paths(&manifest, &loaded.loaded)
        .map_err(|e| syn::Error::new(binding.name.span(), e.to_string()))?;
    for interface in declaration.implements() {
        let id = &loaded.loaded.origins[interface];
        let target: syn::Path = syn::parse_str(
            &organon_cargo::declaration_path(id, &paths[&id.source])
                .map_err(|e| syn::Error::new(binding.name.span(), e.to_string()))?,
        )?;
        let mut fields = vec![];
        for output in &loaded.program.contracts[interface].outputs {
            let field = format_ident!("r#{}", output.name.as_str());
            let source = organon_theoria_compiler::library::interface_output_name(
                declaration,
                interface,
                &output.name,
            );
            let output_marker = format_ident!("{marker}_{source}");
            fields.push(quote!(#field:{let input=self.clone();::organon_theoria::Calculation::new(move |cx|<crate::__organon::#output_marker as ::organon_theoria::Evaluate>::evaluate(&input,cx)).bind(&reactive).expect("native interface output")},));
        }
        provisions.push(quote! {impl ::organon_theoria::Provide<#target> for #ty {fn provide(&self)->#target{let reactive=::organon_theoria::reactive::Reactive::new();#target{#(#fields)*}}}});
    }
    let interpret = quote! {
        fn interpret(text:&str,scope:&::organon_theoria::attribute::Scope)->Result<::organon_theoria::attribute::Attribute<Self::Value>,String> {
            use ::organon_theoria::DefaultParse as _;
            Self::__organon_parse(text,scope)
        }
    };
    Ok(
        quote! {impl ::organon_theoria::Type for crate::__organon::#marker {const ID: &'static str = stringify!(#marker); type Value=#ty; #interpret} #(#provisions)*},
    )
}

pub fn runtime(tokens: TokenStream) -> syn::Result<TokenStream> {
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let path = organon_cargo::CargoSources::default()
        .dependency_rust_path(&manifest, "organon-theoria")
        .map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), e.to_string()))?;
    super::runtime_paths(tokens, &syn::parse_str(&path)?)
}
