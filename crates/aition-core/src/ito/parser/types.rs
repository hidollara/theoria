use super::identifier::{identifier, qualified_name};
use super::rule::{Rule, choice, list, optional, recursive, repeated, sequence, symbol};
use super::util::{delimited, separated};
use crate::ito::lexer::Symbol;
use crate::syntax::{ContractRef, NamedType, Type, TypeParameter};

pub(super) fn ty() -> Rule<Type> {
    recursive(|ty| {
        let terms = sequence![
            term(ty.clone()),
            repeated(sequence![symbol(Symbol::Pipe), term(ty)].map(|(_, ty)| ty)),
        ];
        terms.map(|(first, rest)| {
            if rest.is_empty() {
                return first;
            }
            Type::Union(std::iter::once(first).chain(rest).collect())
        })
    })
}

fn term(ty: Rule<Type>) -> Rule<Type> {
    use Symbol::{Asterisk, CloseParen, OpenParen};

    recursive(|term| {
        let array = sequence![symbol(Asterisk), term];
        let group = delimited(symbol(OpenParen), ty.clone(), symbol(CloseParen));
        choice([
            array.map(|(_, element)| Type::Array(Box::new(element))),
            group.map(|inner| Type::Group(Box::new(inner))),
            function_type(ty.clone()),
            named_type(ty).map(Type::Named),
        ])
    })
}

fn function_type(ty: Rule<Type>) -> Rule<Type> {
    use Symbol::{CloseBracket, CloseParen, Comma, OpenBracket, OpenParen};

    sequence![
        list(
            symbol(OpenBracket),
            ty.clone(),
            symbol(Comma),
            symbol(CloseBracket)
        ),
        delimited(symbol(OpenParen), ty, symbol(CloseParen)),
    ]
    .map(|(parameters, result)| Type::Function {
        parameters,
        result: Box::new(result),
    })
}

fn type_arguments(ty: Rule<Type>) -> Rule<Vec<Type>> {
    use Symbol::{CloseAngle, Comma, OpenAngle};
    list(symbol(OpenAngle), ty, symbol(Comma), symbol(CloseAngle))
}

fn named_type(ty: Rule<Type>) -> Rule<NamedType> {
    sequence![qualified_name(), optional(type_arguments(ty))]
        .map(|(name, arguments)| NamedType { name, arguments })
}

fn contract_ref() -> Rule<ContractRef> {
    sequence![qualified_name(), optional(type_arguments(ty()))]
        .map(|(name, arguments)| ContractRef { name, arguments })
}

pub(super) fn type_parameter() -> Rule<TypeParameter> {
    let bounds = sequence![
        symbol(Symbol::Colon),
        separated(contract_ref(), symbol(Symbol::Ampersand)),
    ];
    sequence![identifier(), optional(bounds)].map(|(name, bounds)| TypeParameter {
        name,
        bounds: bounds.map(|(_, bounds)| bounds).unwrap_or_default(),
    })
}
