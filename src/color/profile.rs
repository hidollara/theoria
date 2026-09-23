use super::{ColorModel, Lab, Rgb, Xyz};

/// Gives a [`ColorModel`] physical color meaning and a path to a profile
/// connection space.
///
/// The concrete transform representation is intentionally added separately;
/// this trait establishes the compile-time relationship between a profile and
/// the coordinate model it accepts.
pub trait ColorProfile: 'static {
    type Model: ColorModel;
}

macro_rules! profile {
    ($name:ident, $model:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorProfile for $name {
            type Model = $model;
        }
    };
}

profile!(Srgb, Rgb);
profile!(LinearSrgb, Rgb);
profile!(DisplayP3, Rgb);
profile!(XyzD50, Xyz);
profile!(XyzD65, Xyz);
profile!(LabD50, Lab);

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_rgb_profile<P: ColorProfile<Model = Rgb>>() {}

    #[test]
    fn profiles_declare_their_coordinate_model() {
        assert_rgb_profile::<Srgb>();
        assert_rgb_profile::<LinearSrgb>();
        assert_rgb_profile::<DisplayP3>();
    }
}
