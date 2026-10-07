pub use theoria_canvas::path::*;

mod flatten;
pub(crate) use flatten::{append_path, append_segment, build_lyon_path, flatten_subpaths};
