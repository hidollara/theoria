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
}

fn marker(binding: &Binding) -> syn::Result<Ident> {
    let origin = &binding.library.loaded.origins[&binding.name.value()];
    let name = organon_cargo::identity(&origin.file, &origin.name)
        .map_err(|e| syn::Error::new(binding.name.span(), e.to_string()))?;
    Ok(format_ident!("{name}"))
}

pub fn implement(item: syn::ItemImpl) -> syn::Result<TokenStream> {
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
        .file
        .definitions()
        .find(|d| d.name() == local)
        .ok_or_else(|| syn::Error::new_spanned(&item.self_ty, "unknown declaration"))?;
    if matches!(definition, organon_core::format::Definition::Element(_)) {
        return Err(syn::Error::new_spanned(
            &item.self_ty,
            "XML elements already define their implementation",
        ));
    }
    let binding = Binding {
        library,
        name: LitStr::new(&local, ident.span()),
        output: None,
        interface: None,
    };
    block::expand(binding, item)
}

fn implement_output(binding: Binding, item: TokenStream) -> syn::Result<TokenStream> {
    let function: syn::ItemFn = syn::parse2(item)?;
    let name = &function.sig.ident;
    let marker = marker(&binding)?;
    let loaded = &binding.library;
    let file = &loaded.loaded.file;
    let declaration = file
        .definitions()
        .find_map(|d| match d {
            organon_core::format::Definition::External(e) if e.name == binding.name.value() => {
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
        if declaration.implementation(&interface_name).is_some() {
            return Err(syn::Error::new(
                interface.span(),
                "interface already maps declared outputs; use implements for a direct Rust implementation",
            ));
        }
        loaded.program.signatures[&interface_name]
            .outputs
            .iter()
            .map(|p| p.name.clone())
            .collect()
    } else {
        declaration.outputs().map(|o| o.port.name.clone()).collect()
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
        let accessor = if spelling == "children" && declaration.children().is_some() {
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
    Ok(quote! {#function
      impl ::organon_theoria::Evaluate for crate::__organon::#output_marker {
       fn evaluate(input:&Self::Input,cx:&::organon_theoria::ReadContext<'_, '_>)->Self::Value {
        let _=(input,cx); #(#reads)* #name(#(#args),*)
       }
      }
    })
}

mod block;

fn implement_value(binding: Binding, item: TokenStream) -> syn::Result<TokenStream> {
    if binding.interface.is_some() || binding.output.is_some() {
        return Err(syn::Error::new(
            binding.name.span(),
            "interface and output require a function implementation",
        ));
    }
    let alias: syn::ItemType = syn::parse2(item)?;
    let loaded = &binding.library;
    let declaration = loaded
        .loaded
        .file
        .definitions()
        .find_map(|d| match d {
            organon_core::format::Definition::External(e)
                if !e.tag && e.name == binding.name.value() =>
            {
                Some(e)
            }
            _ => None,
        })
        .ok_or_else(|| syn::Error::new(binding.name.span(), "not a native value declaration"))?;
    let marker = marker(&binding)?;
    let ty = &alias.ty;
    let mut provisions = vec![];
    let manifest =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml");
    let paths = organon_cargo::CargoSources::default()
        .provider_paths(&manifest, &loaded.loaded)
        .map_err(|e| syn::Error::new(binding.name.span(), e.to_string()))?;
    for interface in declaration.implements() {
        let id = &loaded.loaded.origins[interface];
        let target: syn::Path = syn::parse_str(
            &organon_cargo::declaration_path(id, &paths[&id.file])
                .map_err(|e| syn::Error::new(binding.name.span(), e.to_string()))?,
        )?;
        let mut fields = vec![];
        for output in &loaded.program.signatures[interface].outputs {
            let field = format_ident!("r#{}", output.name);
            let source = declaration
                .implementation(interface)
                .and_then(|i| i.provides.iter().find(|p| p.name == output.name))
                .map(|p| p.reference.clone())
                .unwrap_or_else(|| {
                    organon_theoria_compiler::library::interface_output_name(
                        declaration,
                        interface,
                        &output.name,
                    )
                });
            let output_marker = format_ident!("{marker}_{source}");
            fields.push(quote!(#field:{let input=self.clone();::organon_theoria::Calculation::new(move |cx|<crate::__organon::#output_marker as ::organon_theoria::Evaluate>::evaluate(&input,cx)).bind(&reactive,false).expect("native interface output")},));
        }
        provisions.push(quote! {impl ::organon_theoria::Provide<#target> for #ty {fn provide(&self)->#target{let reactive=::organon_theoria::reactive::Reactive::new();#target{#(#fields)*}}}});
    }
    let interpret = quote! {
        fn reference_only()->bool {#[allow(unused_imports)] use ::organon_theoria::DefaultParse as _;Self::__ORGANON_REFERENCE_ONLY}
        fn interpret(text:&str,scope:&::organon_theoria::attribute::Scope)->Result<::organon_theoria::attribute::Attribute<Self::Value>,String> {
            use ::organon_theoria::DefaultParse as _;
            Self::__organon_parse(text,scope)
        }
    };
    Ok(
        quote! {#alias impl ::organon_theoria::Type for crate::__organon::#marker {type Value=#ty; #interpret} #(#provisions)*},
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
