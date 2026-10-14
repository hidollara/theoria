use organon_theoria::{
    Source,
    attribute::{ParseContext, ParseError, ResolveContext},
};
mod generated {
    organon_theoria::library!(crate::generated);
}

#[organon_theoria::implement]
impl Hidden {
    type Value = f32;
    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<f32>, String> + use<'a>,
        ParseError,
    > {
        let reference = match text.as_bytes() {
            b"literal" => None,
            b"dependency" => match cx.reference::<organon_theoria::types::number>("risk") {
                Ok(r) => Some(r),
                Err(e) => return Err(e),
            },
            _ => return Err(ParseError::new("expected literal or dependency")),
        };
        Ok(move |cx: &mut ResolveContext<'_>| match reference {
            Some(r) => r.resolve(cx),
            None => Ok(Source::from(10.0)),
        })
    }
}

#[organon_theoria::implement]
impl Fault {
    type Value = String;
    #[parse]
    const fn parse<'a>(
        text: &'a str,
        _: &mut ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut ResolveContext<'_>) -> Result<Source<String>, String> + use<'a>,
        ParseError,
    > {
        Ok(move |_: &mut ResolveContext<'_>| Ok(Source::from(text.to_owned())))
    }
}
#[organon_theoria::implement]
impl Risk {
    #[output(name = "n")]
    fn n(input: Input<'_, Self>) -> Result<f32, String> {
        let n = *input.n();
        if n < 0.0 {
            Err("negative".into())
        } else {
            Ok(n)
        }
    }
}

#[test]
fn literal_and_reference_effects_are_independent_of_placement() {
    let reactive = theoria_reactive::Reactive::new();
    let state = reactive.create_state(-1.0f32);
    let probe = generated::Probe::builder()
        .n(state.value())
        .build(&reactive)
        .unwrap();
    let _: Source<generated::Boxed> = probe.direct();
    let _: Source<generated::Boxed> = probe.branch();
    let _: Source<Result<generated::Boxed, String>> = probe.dependent();
    let _: Source<Result<generated::Boxed, String>> = probe.dependent_branch();
    let _: Source<f32> = probe.recovered();
    let _: Source<Result<String, String>> = probe.text();
    let _: Source<f32> = probe.literal();
    let _: Source<Result<f32, String>> = probe.implicit();
    let result = reactive.create_computed(move |cx| {
        let cx = organon_theoria::ReadContext::new(cx);
        let direct = probe.direct().read(&cx).n().read(&cx).into_cloned();
        let branch = probe.branch().read(&cx).n().read(&cx).into_cloned();
        let dependent = probe
            .dependent()
            .read(&cx)
            .as_ref()
            .map(|b| b.n().read(&cx).into_cloned())
            .map_err(Clone::clone);
        let dependent_branch = probe
            .dependent_branch()
            .read(&cx)
            .as_ref()
            .map(|b| b.n().read(&cx).into_cloned())
            .map_err(Clone::clone);
        (
            direct,
            branch,
            dependent,
            dependent_branch,
            probe.recovered().read(&cx).into_cloned(),
            probe.text().read(&cx).into_cloned(),
        )
    });
    assert_eq!(
        *result.snapshot().unwrap(),
        (
            10.0,
            10.0,
            Err("negative".into()),
            Err("negative".into()),
            9.0,
            Err("negative".into())
        )
    );
    state.set(3.0).unwrap();
    assert_eq!(
        *result.snapshot().unwrap(),
        (10.0, 10.0, Ok(3.0), Ok(3.0), 3.0, Ok("n=3, again=3".into()))
    );
}

#[test]
fn forwarding_inferred_failures_preserves_the_source_through_interfaces() {
    use organon_theoria::{Provide, ReadContext};
    let reactive = theoria_reactive::Reactive::new();
    let state = reactive.create_state(Err::<f32, String>("offline".into()));
    let original = Source::from(state.value());
    let forward = generated::FallibleForward::builder()
        .n(original.clone()).build(&reactive).unwrap();
    let interface: generated::FallibleNumber = forward.provide();
    let projection = generated::FallibleProjection::builder()
        .source(forward.clone()).build(&reactive).unwrap();
    let projected_interface: generated::FallibleNumber = projection.provide();
    let read = reactive.create_computed(move |cx| {
        let cx = ReadContext::new(cx);
        let expected = original.read(&cx);
        for source in [forward.n(), interface.n(), projection.n(), projected_interface.n()] {
            let value = source.read(&cx);
            // These outputs must retain the existing Reactive node, not create a copy.
            assert!(std::ptr::eq(&*expected, &*value));
        }
        expected.into_cloned()
    });
    assert_eq!(*read.snapshot().unwrap(), Err("offline".into()));
    state.set(Ok(7.0)).unwrap();
    assert_eq!(*read.snapshot().unwrap(), Ok(7.0));
}
