use theoria_canvas::canvas::Composition;
use theoria_color::ColorSpace;

pub trait CompositionExt {
    fn requires_destination_read_in(&self, working_space: &ColorSpace) -> bool;
}

impl CompositionExt for Composition {
    fn requires_destination_read_in(&self, working_space: &ColorSpace) -> bool {
        self.blend().is_some() || self.composite_space() != working_space
    }
}
