use std::{fs, path::Path, process::Command, time::SystemTime};
#[test]
fn external_library_has_the_same_contract_path_as_the_standard_library() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let target = repository.join("target/organon-library-tests");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = target.join(format!("cases/{stamp}"));
    let provider = root.join("provider");
    let consumer = root.join("consumer");
    for p in [&provider, &consumer] {
        fs::create_dir_all(p.join("src")).unwrap();
    }
    let runtime = repository
        .join("crates/organon-theoria")
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(provider.join("Cargo.toml"),format!("[package]\nname='sample-elements'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\nnative={{package='organon-theoria',path={runtime:?}}}\n")).unwrap();
    fs::write(
        provider.join("organon.xml"),
        r#"<organon version="1">
  <use from="native" name="f32"/>
  <declare name="Pair" tag="false"/>
  <interface name="Numeric">
    <output name="value" type="f32"/>
  </interface>
  <interface name="Other">
    <output name="value" type="f32" fallible="true"/>
  </interface>
  <declare name="Twice">
    <input name="value" type="f32"/>
    <implements interface="Numeric"/>
    <implements interface="Other"/>
  </declare>
  <declare name="Sum">
    <children type="Numeric"/>
    <output name="value" type="f32"/>
  </declare>
  <declare name="Choice">
    <input name="choose" type="f32"/>
    <input name="left" type="f32"/>
    <input name="right" type="f32"/>
    <output name="selected" type="f32"/>
    <output name="alias" type="f32"/>
    <output name="forwarded" type="f32"/>
  </declare>
  <declare name="Holder">
    <input name="choice" type="Choice"/>
    <output name="value" type="f32"/>
  </declare>
  <export name="Pair"/>
  <export name="Numeric"/>
  <export name="Other"/>
  <export name="Twice"/>
  <export name="Sum"/>
  <export name="Choice"/>
  <export name="Holder"/>
</organon>"#,
    )
    .unwrap();
    fs::rename(
        provider.join("organon.xml"),
        provider.join("definitions.xml"),
    )
    .unwrap();
    fs::write(
        provider.join("organon.xml"),
        r#"<organon version="1">
  <import from="./definitions.xml"/>
  <use from="native" name="f32"/>
  <export name="Pair"/>
  <export name="Numeric"/>
  <export name="Other"/>
  <export name="Twice"/>
  <export name="Sum"/>
  <export name="Choice"/>
  <export name="Holder"/>
  <export name="Echo"/>
  <export name="Echo" as="Greeting"/>

  <element name="Private">
    <input name="value" type="f32"/>
    <provide name="value" ref="value"/>
  </element>
  <element name="Echo">
    <input name="value" type="f32"/>
    <provide name="value" ref="value"/>
  </element>
</organon>"#,
    )
    .unwrap();
    fs::write(
        provider.join("src/lib.rs"),
        r#"pub mod placement { pub mod generated { native::library!(crate::placement::generated); pub struct RenderOptions; } }
pub mod controls { pub use crate::placement::generated::{Echo, types, attribute_types}; }
 mod behavior {
 #[native::implement]
 impl Pair {
 type Value = (f32, String);
 // An ordinary helper named parse is not an attribute parser.
 fn parse() {}
 #[parse]
 fn parse_pair(text:&str,cx:&mut native::attribute::ParseContext<'_>)->Result<native::Source<Self::Value>,String> {
     let (number,label)=text.split_once(':').ok_or("expected number:label")?;
     let number=cx.reference::<native::types::f32>(number)?;
     let label=cx.reference::<native::types::String>(label)?;
     Ok(native::Calculation::new(move|cx| (number.read(cx).into_cloned(),label.read(cx).into_cloned())).into_inline())
 }
 }
 #[native::implement]
 impl Numeric {
 #[parse]
 fn parse(text:&str,cx:&mut native::attribute::ParseContext<'_>)->Result<native::Source<Self>,String> {
 cx.reference::<Self>(text.strip_prefix('@').ok_or("expected @ reference")?)
 }
 }

 #[native::implement]
 impl Twice {
 #[output(interface="Numeric",name="value")]
 fn twice(input:Input<'_,Self>)->f32 {let value=*input.value();value*2.0}
 #[output(interface="Other",name="value")]
 fn other()->Result<f32,&'static str> {Ok(99.0)}
 }
 #[native::implement]
 impl Sum {
 #[output(name="value")]
 fn sum(input:Input<'_,Self>)->f32 {input.children().iter().map(|v|*input.read(&v.value())).sum()}
 }
 #[native::implement]
 impl Choice {
 #[parse]
 fn parse(text:&str,cx:&mut native::attribute::ParseContext<'_>)->Result<native::Source<Self::Value>,String> {
     cx.reference::<Self>(text.strip_prefix('@').ok_or("expected @ choice")?)
 }
 #[output(name="selected")]
 #[output(name="alias")]
 fn selected(input:Input<'_,Self>)->f32 {
     if *input.choose() > 0.0 { *input.left() } else { *input.right() }
 }
 forward!(left => forwarded);
 }
 #[native::implement]
 impl Holder {
 #[output(name="value")]
 fn value(input:Input<'_,Self>)->f32 {
     *input.read(&input.choice().selected())
 }
 }
 }
 "#,
    )
    .unwrap();
    let provider_path = provider.to_string_lossy().replace('\\', "/");
    let reactive = repository
        .join("crates/theoria-reactive")
        .to_string_lossy()
        .replace('\\', "/");
    let component = repository
        .join("crates/theoria-component")
        .to_string_lossy()
        .replace('\\', "/");
    let app = repository
        .join("apps/organon")
        .to_string_lossy()
        .replace('\\', "/");
    let cargo = repository
        .join("crates/organon-cargo")
        .to_string_lossy()
        .replace('\\', "/");
    fs::write(consumer.join("Cargo.toml"),format!("[package]\nname='library-consumer'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\norganon-theoria={{path={runtime:?}}}\norganon-app={{path={app:?}}}\norganon-cargo={{path={cargo:?}}}\nwidgets={{package='sample-elements',path={provider_path:?}}}\ntheoria-reactive={{path={reactive:?}}}\ntheoria-component={{path={component:?}}}\n")).unwrap();
    fs::write(
        consumer.join("organon.xml"),
        r#"<organon version="1">
  <import from="organon-theoria"/>
  <import from="widgets"/>
  <element name="Example">
    <input name="value" type="f32"/>
    <bind value="sum">
      <Sum>
        <Twice value="{value}"/>
        <Twice value="3"/>
      </Sum>
    </bind>
    <provide name="sum" ref="sum"/>
    <let name="echo">
      <Echo value="{value}"/>
    </let>
    <provide name="echo" ref="echo"/>
  </element>
  <element name="ChoiceView">
    <input name="choose" type="f32"/>
    <input name="left" type="f32"/>
    <input name="right" type="f32"/>
    <bind selected="selected" alias="alias" forwarded="forwarded">
      <Choice choose="{choose}" left="{left}" right="{right}"/>
    </bind>
    <provide name="selected" ref="selected"/>
    <provide name="alias" ref="alias"/>
    <provide name="forwarded" ref="forwarded"/>
  </element>
  <element name="ReferenceView">
    <bind value="value">
      <Holder choice="@chosen"/>
    </bind>
    <let name="chosen">
      <Choice choose="1" left="7" right="9"/>
    </let>
    <provide name="value" ref="value"/>
  </element>
  <export name="Example"/>
  <export name="ChoiceView"/>
<element name="Preview">
  <let name="object">
    <Twice value="4"/>
  </let>
  <provide name="object" ref="object"/>
  <bind value="value">
    <Sum>
      <Twice value="4"/>
      <Twice value="3"/>
    </Sum>
  </bind>
  <provide name="value" ref="value"/>
  <let name="choice">
    <Choice choose="1" left="7" right="9"/>
  </let>
  <bind value="held">
    <Holder choice="@choice"/>
  </bind>
  <provide name="held" ref="held"/>
</element>
  <export name="ReferenceView"/>
</organon>"#,
    )
    .unwrap();
    fs::write(consumer.join("src/main.rs"),r###"pub mod generated { organon_theoria::library!(crate::generated); }
mod preview { organon_app::preview_library!("organon.xml"); }
use widgets::controls::types as widget_contracts;
use generated::{Example,ChoiceView,ReferenceView};
 fn main(){let reactive=theoria_reactive::Reactive::new();
 let echo=widgets::controls::Echo::builder().value(2.0).build(&reactive).unwrap();
 let same: widgets::controls::types::Echo=echo;
 let _: widgets::controls::Echo = same.clone();
 let _: widgets::controls::types::Greeting = same.clone();
 assert_eq!(same.value_fixed(),Some(&2.0));let value=Example::builder().value(4.0).build(&reactive).unwrap();assert_eq!(value.sum_fixed(),Some(&14.0));
 let echo: &widgets::controls::Echo=value.echo_fixed().unwrap();
 assert_eq!(echo.value_fixed(),Some(&4.0));
 assert_eq!(ReferenceView::builder().build(&reactive).unwrap().value_fixed(),Some(&7.0));
 let choose=reactive.create_state(1.0_f32);
 let left=reactive.create_state(10.0_f32);
 let right=reactive.create_state(20.0_f32);
 let fixed=ChoiceView::builder().choose(1.0).left(5.0).right(right.value()).build(&reactive).unwrap();
 assert_eq!(fixed.selected_fixed(),Some(&5.0));
 let choice=ChoiceView::builder().choose(choose.value()).left(left.value()).right(right.value()).build(&reactive).unwrap();
 let value=choice.selected();
 let count=std::rc::Rc::new(std::cell::Cell::new(0));let calls=count.clone();
 let selected=reactive.create_computed(move|cx|{calls.set(calls.get()+1);*value.read(&organon_theoria::ReadContext::new(cx))});
 assert_eq!(*selected.snapshot().unwrap(),10.0);
 right.set(30.0).unwrap();assert_eq!(*selected.snapshot().unwrap(),10.0);assert_eq!(count.get(),1);
 choose.set(0.0).unwrap();assert_eq!(*selected.snapshot().unwrap(),30.0);
 left.set(40.0).unwrap();assert_eq!(*selected.snapshot().unwrap(),30.0);assert_eq!(count.get(),2);
 let forwarded=choice.forwarded();let read=reactive.create_computed(move|cx|*forwarded.read(&organon_theoria::ReadContext::new(cx)));
 assert_eq!(*read.snapshot().unwrap(),40.0);
 use organon_theoria::Type;
 let mut parser_scope=organon_theoria::attribute::Scope::new(organon_theoria::attribute_types());
 parser_scope.declare_typed::<f32>("number");parser_scope.declare_typed::<String>("label");
 let parsed=widgets::controls::types::Pair::interpret("number:label",&parser_scope).unwrap();
 assert!(widgets::controls::types::Pair::interpret("label:number",&parser_scope).is_err());
 parser_scope.set("number",organon_theoria::Source::from(organon_theoria::attribute::Value::new(3.0_f32)));
 parser_scope.set("label",organon_theoria::Source::from(organon_theoria::attribute::Value::new("three".to_owned())));
 let pair={ let source = parsed.source; organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned()).bind(&reactive, true).unwrap() };
 assert_eq!(pair.as_fixed().unwrap(),&(3.0,"three".to_owned()));
 let mut scope=organon_theoria::attribute::Scope::new(widgets::controls::attribute_types());
 scope.declare_typed::<widget_contracts::Numeric>("sample");
 assert!(widget_contracts::Numeric::interpret("@sample",&scope).is_ok());
 assert!(widget_contracts::Numeric::interpret("sample",&scope).is_err());
 scope.declare_typed::<widget_contracts::Other>("other");
 assert!(widget_contracts::Other::interpret("other",&scope).is_ok());
 assert!(widget_contracts::Other::interpret("sample",&scope).is_err());
 assert!(widget_contracts::Other::interpret("@sample",&scope).is_err());
 let loaded=organon_cargo::load(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("organon.xml")).unwrap();
 let mut registry=preview::registry();
 for (local,id) in &loaded.loaded.origins {
     let identity=organon_cargo::identity(&id.file,&id.name).unwrap();
     let _=registry.alias(local,&identity);
 }
 let outputs=registry.evaluate(&loaded.program,"Preview",Default::default()).unwrap();
 assert_eq!(outputs["value"].as_ref().unwrap().get::<f32>().unwrap(),14.0);
 assert_eq!(outputs["held"].as_ref().unwrap().get::<f32>().unwrap(),7.0);
 let object=outputs["object"].as_ref().unwrap().get::<organon_app::evaluation::Object>().unwrap();
 assert!(object.outputs.is_empty());
 let mut values=object.interfaces.values().map(|ports|ports["value"].as_ref().unwrap().get::<f32>().unwrap()).collect::<Vec<_>>();
 values.sort_by(|a,b|a.partial_cmp(b).unwrap());
 assert_eq!(values[1],99.0);
 assert_eq!(values.len(),2);

}
 "###).unwrap();
    let run = || {
        Command::new(env!("CARGO"))
            .args(["run", "--offline", "--quiet", "--manifest-path"])
            .arg(consumer.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", repository.join("target"))
            .output()
            .unwrap()
    };
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The public declaration list must not expose definitions omitted by export.
    let consumer_source = fs::read_to_string(consumer.join("src/main.rs")).unwrap();
    fs::write(
        consumer.join("src/main.rs"),
        format!("{consumer_source}\ntype Hidden = widgets::controls::types::Private;\n"),
    )
    .unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot find type `Private`"));
    fs::write(consumer.join("src/main.rs"), consumer_source).unwrap();

    // The consumer has no direct dependency on the original provider. Follow
    // the intermediate library's connector and its exported dependency links.
    let relay = root.join("relay");
    let indirect = root.join("indirect");
    for path in [&relay, &indirect] {
        fs::create_dir_all(path.join("src")).unwrap();
    }
    fs::write(relay.join("Cargo.toml"), format!(
        "[package]\nname='relay'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\norganon-theoria={{path={runtime:?}}}\nwidgets={{package='sample-elements',path='../provider'}}\n"
    )).unwrap();
    fs::write(
        relay.join("organon.xml"),
        r#"<organon version="1">
  <use from="widgets" name="Echo"/>
  <export name="Echo"/>
</organon>"#,
    )
    .unwrap();
    fs::write(
        relay.join("src/lib.rs"),
        "pub mod bridge { organon_theoria::library!(crate::bridge); }",
    )
    .unwrap();
    fs::write(indirect.join("Cargo.toml"), format!(
        "[package]\nname='indirect'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\norganon-theoria={{path={runtime:?}}}\nrelay={{path='../relay'}}\n"
    )).unwrap();
    fs::write(
        indirect.join("organon.xml"),
        r#"<organon version="1">
  <use from="relay" name="Echo"/>
  <export name="Echo"/>
</organon>"#,
    )
    .unwrap();
    fs::write(
        indirect.join("src/main.rs"),
        r#"
organon_theoria::library!(crate);
fn main() {
    let reactive = organon_theoria::reactive::Reactive::new();
    let value: relay::bridge::Echo = Echo::builder().value(9.0).build(&reactive).unwrap();
    let _: types::Echo = value.clone();
    assert_eq!(value.value_fixed(), Some(&9.0));
}
"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO"))
        .args(["run", "--offline", "--quiet", "--manifest-path"])
        .arg(indirect.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", repository.join("target"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = fs::read_to_string(provider.join("src/lib.rs")).unwrap();
    // Moving the provider's Rust module only changes its own placement argument.
    // Neither the consumer XML nor its Rust connector calls know that location.
    fs::write(
        provider.join("src/lib.rs"),
        source.replace("placement", "relocated"),
    )
    .unwrap();
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::write(
        provider.join("src/lib.rs"),
        source.replace("value*2.0", "value*3.0"),
    )
    .unwrap();
    fs::write(
        consumer.join("src/main.rs"),
        fs::read_to_string(consumer.join("src/main.rs"))
            .unwrap()
            .replace("14.0", "21.0"),
    )
    .unwrap();
    let output = run();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let implementation = fs::read_to_string(provider.join("src/lib.rs")).unwrap();
    fs::write(
        provider.join("src/lib.rs"),
        implementation
            .replace(
                "fn twice(input:Input<'_,Self>)->f32",
                "fn twice(input:Input<'_,Self>)->bool",
            )
            .replace("value*3.0}", "value>0.0}"),
    )
    .unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mismatched types"));
    for (from, to, message) in [
        (
            "native::library!(crate::placement::generated);",
            "native::library!(crate::placement);",
            "library! path must match its containing module",
        ),
        (
            "native::library!(crate::placement::generated);",
            "native::library!(self);",
            "library! requires a crate-rooted module path",
        ),
        (
            "pub mod controls {",
            "mod duplicate { native::library!(crate::duplicate); } pub mod controls {",
            "organon` is defined multiple times",
        ),
        (
            "interface=\"Numeric\"",
            "interface=\"Missing\"",
            "does not implement",
        ),
        ("name=\"value\"", "name=\"missing\"", "unknown output"),
        ("#[parse]", "#[parse]\n #[parse]", "only one #[parse]"),
        (
            "#[parse]",
            "#[parse]\n #[output(name=\"text\")]",
            "parser cannot also be an output",
        ),
        (
            "#[parse]",
            "#[parse(unexpected)]",
            "#[parse] takes no arguments",
        ),
        (
            "native::library!(crate::placement::generated);",
            "native::library!();",
            "library! requires its containing module path",
        ),
        (
            "pub mod controls {",
            "pub use placement::generated::Private; pub mod controls {",
            "unresolved import",
        ),
        (
            "#[native::implement]",
            "#[native::implement(\"organon.xml\",coerce=convert)]",
            "implement takes no arguments",
        ),
    ] {
        fs::write(
            provider.join("src/lib.rs"),
            implementation.replacen(from, to, 1),
        )
        .unwrap();
        let output = run();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
    }
    fs::write(
        provider.join("src/lib.rs"),
        implementation.replace("#[output(interface=\"Numeric\",name=\"value\")]", ""),
    )
    .unwrap();
    let output = run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Evaluate"));
}
