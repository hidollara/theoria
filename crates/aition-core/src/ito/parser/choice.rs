/// Try a nonempty list of candidates in order, stopping at a match or error.
/// All candidates must return the same output type.
macro_rules! choice {
    (@pairs $last_separator:literal; $first:expr, $second:expr, $($rest:expr),+) => {
        $crate::ito::parser::choice!(@pairs ", or ";
            $crate::ito::parser::or($first, $second).separator(", "), $($rest),+)
    };
    (@pairs $last_separator:literal; $first:expr, $second:expr) => {
        $crate::ito::parser::or($first, $second).separator($last_separator)
    };
    (@pairs $last_separator:literal; $last:expr) => { $last };
    ($($rule:expr),+ $(,)?) => {
        $crate::ito::parser::choice!(@pairs " or "; $($rule),+)
    };
}

pub(crate) use choice;

#[cfg(test)]
mod tests;
