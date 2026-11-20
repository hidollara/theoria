use super::Rule;

pub fn pair<A: 'static, B: 'static>(left: Rule<A>, right: Rule<B>) -> Rule<(A, B)> {
    Rule::new(move |tokens| Ok((left.apply(tokens)?, right.apply(tokens)?)))
}

/// Build nested pairs and unpack their values into one flat tuple.
macro_rules! sequence {
    (@pairs $rule:expr, $pattern:pat, [$($values:ident),+];) => {
        $rule.map(|$pattern| ($($values,)+))
    };
    (@pairs $rule:expr, $pattern:pat, [$($values:ident),+]; $next:expr $(, $rest:expr)*) => {
        sequence!(@pairs $crate::ito::parser::rule::sequence::pair($rule, $next),
            ($pattern, next), [$($values,)+ next]; $($rest),*)
    };
    ($first:expr, $($rest:expr),+ $(,)?) => {
        sequence!(@pairs $first, first, [first]; $($rest),+)
    };
}

pub(crate) use sequence;
