use organon_theoria::{
    Source, Type,
    attribute::{ParseBuffer, ParseContext, ParseError, ResolveContext},
};
use std::cell::Cell;

mod generated {
    organon_theoria::library!(crate::generated);
}

#[derive(Clone, Debug)]
pub struct Number(f32);

thread_local! {
    static BUILDS: Cell<usize> = const { Cell::new(0) };
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

struct Literal(Number);
impl Drop for Literal {
    fn drop(&mut self) {
        DROPS.set(DROPS.get() + 1);
    }
}

#[organon_theoria::implement]
impl StaticValue {
    type Value = Number;

    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self::Value>, String> + use<'a>,
        ParseError,
    > {
        let reference = match organon_theoria::attribute::braced_reference(text) {
            Some(name) => match cx.reference::<Self>(name) {
                Ok(reference) => Some(reference),
                Err(error) => return Err(error),
            },
            None => {
                if !matches!(text.as_bytes(), b"seven") {
                    return Err(ParseError::new("expected seven"));
                }
                None
            }
        };
        let literal = Literal(Number(7.0));
        Ok(move |cx: &mut ResolveContext<'_>| {
            // Capture the whole Drop value, not just its Copy payload.
            let value = literal.0.clone();
            drop(literal);
            match reference {
                Some(reference) => reference.resolve(cx),
                None => Ok(Source::from(value)),
            }
        })
    }
}

#[organon_theoria::implement]
impl RuntimeValue {
    type Value = Vec<f32>;

    #[parse]
    const fn parse<'a, const N: usize>(
        text: &'a str,
        _: &mut ParseContext<'_>,
        mut values: ParseBuffer<'a, f32, N>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self::Value>, String> + use<'a, N>,
        ParseError,
    > {
        let mut start = 0;
        let mut end = 0;
        while end <= text.len() {
            if end == text.len() || text.as_bytes()[end] == b',' {
                let token = text.split_at(end).0.split_at(start).1;
                let value = match token.as_bytes() {
                    b"1" => 1.0,
                    b"2" => 2.0,
                    b"3" => 3.0,
                    _ => return Err(ParseError::new("invalid factors")),
                };
                if let Err(e) = values.push(value) {
                    return Err(e);
                }
                start = end + 1;
            }
            end += 1;
        }
        Ok(move |_: &mut ResolveContext<'_>| {
            BUILDS.set(BUILDS.get() + 1);
            Ok(Source::from(values.iter().copied().collect::<Vec<_>>()))
        })
    }
}

#[organon_theoria::implement]
impl Digits {
    type Value = Vec<u8>;

    #[parse]
    const fn parse<'a, const N: usize>(
        text: &'a str,
        _: &mut ParseContext<'_>,
        mut digits: ParseBuffer<'a, u8, N>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<Self::Value>, String> + use<'a, N>,
        ParseError,
    > {
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if !bytes[i].is_ascii_digit() {
                return Err(ParseError::new("expected a digit"));
            }
            if let Err(error) = digits.push(bytes[i] - b'0') {
                return Err(error);
            }
            i += 1;
        }
        Ok(move |_: &mut ResolveContext<'_>| {
            Ok(Source::from(digits.iter().copied().collect::<Vec<_>>()))
        })
    }
}

#[organon_theoria::implement]
impl DigitSum {
    #[output(name = "value")]
    fn value(input: Input<'_, Self>) -> f32 {
        input.digits().iter().map(|n| *n as f32).sum()
    }
}

#[test]
fn library_defined_buffered_parser_works_without_compiler_knowledge() {
    let reactive = theoria_reactive::Reactive::new();
    let value = generated::Buffered::builder().build(&reactive).unwrap();
    assert_eq!(value.value_fixed(), Some(&45.0));
    let scope = organon_theoria::attribute::Scope::default();
    for text in [String::new(), "9876543210".repeat(100)] {
        let parsed = generated::types::Digits::interpret(&text, &scope).unwrap();
        assert_eq!(
            parsed.source.as_fixed().unwrap(),
            &text.bytes().map(|b| b - b'0').collect::<Vec<_>>()
        );
    }
    assert!(generated::types::Digits::interpret("12x", &scope).is_err());
}

#[organon_theoria::implement]
impl Sum {
    #[output(name = "value")]
    fn value(input: Input<'_, Self>) -> f32 {
        input.base().0 * input.factors().iter().sum::<f32>()
    }
}

#[organon_theoria::implement]
impl Pass {
    forward!(value => value);
}

#[test]
fn constructors_run_once_and_sources_keep_updating() {
    BUILDS.set(0);
    DROPS.set(0);
    let reactive = theoria_reactive::Reactive::new();
    let seed = reactive.create_state(Number(2.0));
    let scene = generated::Scene::builder()
        .seed(seed.value())
        .build(&reactive)
        .unwrap();
    assert_eq!(BUILDS.get(), 1);
    assert_eq!(DROPS.get(), 1);
    let value = scene.value();
    let read = reactive.create_computed(move |cx| {
        value
            .read(&organon_theoria::ReadContext::new(cx))
            .into_cloned()
    });
    assert_eq!(*read.snapshot().unwrap(), 10.0);
    seed.set(Number(4.0)).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 20.0);
    assert_eq!(BUILDS.get(), 1);
    assert_eq!(DROPS.get(), 1);
    let fixed = generated::Constant::builder().build(&reactive).unwrap();
    assert_eq!(fixed.value_fixed(), Some(&7.0));
    assert_eq!(BUILDS.get(), 2);
}

#[test]
fn dynamic_parse_errors_and_construction_cycles_are_reported() {
    let reactive = theoria_reactive::Reactive::new();
    let scope = organon_theoria::attribute::Scope::default();
    assert!(generated::types::RuntimeValue::interpret("invalid", &scope).is_err());
    let error = generated::Cycle::builder().build(&reactive).unwrap_err();
    assert!(error.to_string().contains("cyclic binding"));
}

#[test]
fn const_parser_can_also_interpret_dynamic_text() {
    let scope = organon_theoria::attribute::Scope::new(generated::attribute_types());
    let text = String::from("seven");
    let value = generated::types::StaticValue::interpret(&text, &scope).unwrap();
    assert_eq!(value.source.as_fixed().unwrap().0, 7.0);
    assert!(generated::types::StaticValue::interpret("invalid", &scope).is_err());
}
