//! Renderer-owned assembly of composition WGSL with a supplied blend operation.
//! Used at build time and runtime so both compilation paths use the same layout.

const CONTEXT: &str = include_str!("../shaders/composition_context.wgsl");
const FRAGMENT: &str = include_str!("../shaders/composition_fragment.wgsl");

/// Used when a composition has no blend operation. The fragment shader does
/// not call it in that case, but WGSL still needs a complete module.
pub const IDENTITY_BODY: &str = "return source;";

pub(super) fn operation(helpers: &str, body: &str) -> String {
    let fragments = [
        helpers,
        "fn theoria_blend(backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> {",
        body,
        "}",
    ];
    let capacity = fragments.iter().map(|fragment| fragment.len() + 1).sum();
    let mut source = String::with_capacity(capacity);
    for fragment in fragments {
        source.push_str(fragment);
        source.push('\n');
    }
    source
}

pub fn assemble(binding: &str, helpers: &str, body: &str) -> String {
    let conversions = r#"
fn working_to_composite(value: vec3<f32>) -> vec3<f32> { return value; }
fn composite_to_working(value: vec3<f32>) -> vec3<f32> { return value; }
fn working_to_blend(value: vec3<f32>) -> vec3<f32> { return value; }
fn blend_to_composite(value: vec3<f32>) -> vec3<f32> { return value; }
"#;
    assemble_with_conversions(binding, helpers, body, conversions)
}

/// Assembles a module whose color conversions have been lowered from spaces.
pub fn assemble_with_conversions(
    binding: &str,
    helpers: &str,
    body: &str,
    conversions: &str,
) -> String {
    let context = CONTEXT.replace("// @theoria-rgb-conversions", conversions);
    let operation = operation(helpers, body);
    let fragments = [&*context, binding, &operation, FRAGMENT];
    let capacity = fragments.iter().map(|fragment| fragment.len() + 1).sum();
    let mut source = String::with_capacity(capacity);
    for fragment in fragments {
        source.push_str(fragment);
        source.push('\n');
    }
    source
}
