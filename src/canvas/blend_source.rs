//! Assembles one WGSL module from the shared composition code and one blend
//! operation. This file is also included by build.rs so both compilation
//! paths use exactly the same source layout.

const COMMON: &str = include_str!("../../shaders/canvas_renderer/blend_common.wgsl");
pub(crate) const BUILTIN: &str = include_str!("../../shaders/canvas_renderer/blend_builtin.wgsl");
const FRAGMENT: &str = include_str!("../../shaders/canvas_renderer/blend_fragment.wgsl");

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

pub(crate) fn assemble(binding: Binding, operation: &str) -> String {
    let fragments = [COMMON, binding.declaration(), operation, FRAGMENT];
    let capacity = fragments.iter().map(|fragment| fragment.len() + 1).sum();
    let mut source = String::with_capacity(capacity);
    for fragment in fragments {
        source.push_str(fragment);
        source.push('\n');
    }
    source
}
