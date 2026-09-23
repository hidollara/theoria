//! Assembles a composition WGSL module with a supplied blend operation.
//! Also included by build.rs so both compilation paths use the same layout.

const CONTEXT: &str = include_str!("../../shaders/canvas_renderer/composition_context.wgsl");
const FRAGMENT: &str = include_str!("../../shaders/canvas_renderer/composition_fragment.wgsl");

/// Used when a composition has no blend operation. The fragment shader does
/// not call it in that case, but WGSL still needs a complete module.
pub(crate) const IDENTITY_BODY: &str = "return source;";

#[derive(Clone, Copy)]
pub(crate) enum Binding {
    WebGpu,
    Vulkan,
}

impl Binding {
    fn declaration(self) -> &'static str {
        match self {
            Self::WebGpu => "@group(2) @binding(0) var<uniform> operation: CompositionOperation;",
            Self::Vulkan => "var<immediate> operation: CompositionOperation;",
        }
    }
}

pub(crate) fn operation(helpers: &str, body: &str) -> String {
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

pub(crate) fn assemble(binding: Binding, helpers: &str, body: &str) -> String {
    let operation = operation(helpers, body);
    let context = crate::rgb_shader::assemble(CONTEXT);
    let fragments = [&*context, binding.declaration(), &operation, FRAGMENT];
    let capacity = fragments.iter().map(|fragment| fragment.len() + 1).sum();
    let mut source = String::with_capacity(capacity);
    for fragment in fragments {
        source.push_str(fragment);
        source.push('\n');
    }
    source
}
