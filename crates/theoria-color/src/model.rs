use super::{UnitF32, UnitF64};

/// An authored coordinate representation accepted by a color model.
///
/// This contract validates storage without decoding or converting its values.
pub trait ColorCoordinates<M: ColorModel>: 'static {
    fn is_finite(&self) -> bool;
}

/// Coordinates that can be expanded for an explicit CPU reference conversion.
/// Rendering a color does not require this additional capability.
pub trait ReferenceCoordinates<M: ColorModel>: ColorCoordinates<M> {
    fn to_reference(&self) -> M::ReferenceCoordinates;
}

/// Defines the structure and ordering of a color profile's coordinates.
///
/// A model assigns no physical color meaning by itself. Primaries, transfer
/// functions, white points, matrices, and lookup tables belong to a profile.
pub trait ColorModel: Sized + 'static {
    type ReferenceCoordinates: ColorCoordinates<Self> + AsRef<[f64]>;

    const COMPONENTS: usize;
}

macro_rules! coordinate_representations {
    ($model:ty, $components:expr) => {
        impl ColorCoordinates<$model> for [f32; $components] {
            fn is_finite(&self) -> bool {
                self.iter().all(|value| value.is_finite())
            }
        }
        impl ReferenceCoordinates<$model> for [f32; $components] {
            fn to_reference(&self) -> [f64; $components] {
                self.map(f64::from)
            }
        }
        impl ColorCoordinates<$model> for [f64; $components] {
            fn is_finite(&self) -> bool {
                self.iter().all(|value| value.is_finite())
            }
        }
        impl ReferenceCoordinates<$model> for [f64; $components] {
            fn to_reference(&self) -> [f64; $components] {
                *self
            }
        }
        impl ColorCoordinates<$model> for [UnitF32; $components] {
            fn is_finite(&self) -> bool {
                true
            }
        }
        impl ReferenceCoordinates<$model> for [UnitF32; $components] {
            fn to_reference(&self) -> [f64; $components] {
                self.map(|value| f64::from(value.get()))
            }
        }
        impl ColorCoordinates<$model> for [UnitF64; $components] {
            fn is_finite(&self) -> bool {
                true
            }
        }
        impl ReferenceCoordinates<$model> for [UnitF64; $components] {
            fn to_reference(&self) -> [f64; $components] {
                self.map(UnitF64::get)
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
        impl ReferenceCoordinates<$model> for [$sample; $components] {
            fn to_reference(&self) -> [f64; $components] {
                self.map(|value| f64::from(value) / f64::from(<$sample>::MAX))
            }
        }
    };
}

macro_rules! fixed_model {
    ($name:ident, $components:expr) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorModel for $name {
            type ReferenceCoordinates = [f64; $components];

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

// Integer code values have a reference expansion only for RGB at present.
integer_coordinates!(Rgb, 3, u8);
integer_coordinates!(Rgb, 3, u16);

/// A device color model with a statically known number of channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DeviceN<const N: usize>;

impl<const N: usize> ColorModel for DeviceN<N> {
    type ReferenceCoordinates = [f64; N];

    const COMPONENTS: usize = N;
}

macro_rules! device_coordinates {
    ($sample:ty, $valid:expr, $reference:expr) => {
        impl<const N: usize> ColorCoordinates<DeviceN<N>> for [$sample; N] {
            fn is_finite(&self) -> bool {
                self.iter().all($valid)
            }
        }
        impl<const N: usize> ReferenceCoordinates<DeviceN<N>> for [$sample; N] {
            fn to_reference(&self) -> [f64; N] {
                self.map($reference)
            }
        }
    };
}

device_coordinates!(f32, |value: &f32| value.is_finite(), f64::from);
device_coordinates!(f64, |value: &f64| value.is_finite(), |value: f64| value);
device_coordinates!(UnitF32, |_: &UnitF32| true, |value: UnitF32| f64::from(
    value.get()
));
device_coordinates!(UnitF64, |_: &UnitF64| true, UnitF64::get);

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
    fn reference_expansion_is_separate_from_storage_validation() {
        assert_eq!(
            <[u8; 3] as ReferenceCoordinates<Rgb>>::to_reference(&[0, 128, 255]),
            [0.0, 128.0 / 255.0, 1.0]
        );
        assert_eq!(
            <[f32; 3] as ReferenceCoordinates<Xyz>>::to_reference(&[2.0, 0.0, -1.0]),
            [2.0, 0.0, -1.0]
        );
        assert!(!<[f32; 3] as ColorCoordinates<Lab>>::is_finite(&[
            f32::NAN,
            0.0,
            0.0
        ]));
    }
}
