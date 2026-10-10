pub use theoria_canvas::path::*;

mod flatten;
pub use flatten::flatten_subpaths;
pub(crate) use flatten::{append_path, append_segment, build_lyon_path};
