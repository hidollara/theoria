/// An authored coordinate representation accepted by a color model.
///
/// This contract validates storage without decoding or converting its values.
pub trait ColorCoordinates<M: ColorModel>: 'static {
    fn is_finite(&self) -> bool;
}

/// Defines the structure and ordering of a color profile's coordinates.
///
/// A model assigns no physical color meaning by itself. Primaries, transfer
/// functions, white points, matrices, and lookup tables belong to a profile.
pub trait ColorModel: Sized + 'static {
    const COMPONENTS: usize;
}

macro_rules! coordinate_representations {
    ($model:ty, $components:expr) => {
        impl ColorCoordinates<$model> for [f32; $components] {
            fn is_finite(&self) -> bool {
                self.iter().all(|value| value.is_finite())
            }
        }
        impl ColorCoordinates<$model> for [f64; $components] {
            fn is_finite(&self) -> bool {
                self.iter().all(|value| value.is_finite())
            }
        }
    };
}

macro_rules! integer_coordinates {
    ($model:ty, $components:expr, $sample:ty) => {
        impl ColorCoordinates<$model> for [$sample; $components] {
            fn is_finite(&self) -> bool {
                true
            }
        }
    };
}

macro_rules! fixed_model {
    ($name:ident, $components:expr) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorModel for $name {
            const COMPONENTS: usize = $components;
        }

        coordinate_representations!($name, $components);
    };
}

fixed_model!(Rgb, 3);
fixed_model!(Cmyk, 4);
fixed_model!(Xyz, 3);
fixed_model!(Lab, 3);
fixed_model!(Ycbcr, 3);

// Integer coordinate storage is currently defined only for RGB.
integer_coordinates!(Rgb, 3, u8);
integer_coordinates!(Rgb, 3, u16);

/// A device color model with a statically known number of channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DeviceN<const N: usize>;

impl<const N: usize> ColorModel for DeviceN<N> {
    const COMPONENTS: usize = N;
}

macro_rules! device_coordinates {
    ($sample:ty, $valid:expr) => {
        impl<const N: usize> ColorCoordinates<DeviceN<N>> for [$sample; N] {
            fn is_finite(&self) -> bool {
                self.iter().all($valid)
            }
        }
    };
}

device_coordinates!(f32, |value: &f32| value.is_finite());
device_coordinates!(f64, |value: &f64| value.is_finite());

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts<M: ColorModel, C: ColorCoordinates<M>>() {}

    #[test]
    fn models_accept_whole_coordinate_representations() {
        accepts::<Rgb, [u8; 3]>();
        accepts::<Cmyk, [f32; 4]>();
        accepts::<DeviceN<7>, [f64; 7]>();
        assert_eq!(Rgb::COMPONENTS, 3);
        assert_eq!(DeviceN::<7>::COMPONENTS, 7);
    }

    #[test]
    fn coordinate_validation_rejects_non_finite_values() {
        assert!(!<[f32; 3] as ColorCoordinates<Lab>>::is_finite(&[
            f32::NAN,
            0.0,
            0.0
        ]));
    }
}
