use super::{Sample, UnitF32, UnitF64};

/// An ordered set of scalar coordinates belonging to a [`ColorModel`].
pub trait ColorCoordinates: 'static {
    type Sample: Sample;

    const COMPONENTS: usize;

    fn as_slice(&self) -> &[Self::Sample];
}

impl<S: Sample, const N: usize> ColorCoordinates for [S; N] {
    type Sample = S;

    const COMPONENTS: usize = N;

    fn as_slice(&self) -> &[S] {
        self
    }
}

/// Defines the structure and ordering of a color profile's coordinates.
///
/// A model assigns no physical color meaning by itself. Primaries, transfer
/// functions, white points, matrices, and lookup tables belong to a profile.
pub trait ColorModel: 'static {
    type Coordinates<S: Sample>: ColorCoordinates<Sample = S>;

    const COMPONENTS: usize;

    /// Maps coordinate samples without changing their model or channel order.
    fn map_samples<S: Sample, T: Sample>(
        coordinates: &Self::Coordinates<S>,
        map: impl FnMut(S) -> T,
    ) -> Self::Coordinates<T>;
}

macro_rules! fixed_model {
    ($name:ident, $components:expr) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorModel for $name {
            type Coordinates<S: Sample> = [S; $components];

            const COMPONENTS: usize = $components;

            fn map_samples<S: Sample, T: Sample>(
                coordinates: &Self::Coordinates<S>,
                map: impl FnMut(S) -> T,
            ) -> Self::Coordinates<T> {
                coordinates.map(map)
            }
        }
    };
}

fixed_model!(Rgb, 3);
fixed_model!(Cmyk, 4);
fixed_model!(Xyz, 3);
fixed_model!(Lab, 3);
fixed_model!(Ycbcr, 3);

/// A sample representation with a defined expansion to reference coordinates
/// for this model. Floating-point values are already coordinates; integer
/// code values are currently defined only for RGB, where they are normalized.
pub trait ReferenceSample<M: ColorModel>: Sample {
    /// Expands an authored coordinate for an explicit CPU reference conversion.
    fn to_reference_f64(self) -> f64;
}

impl<M: ColorModel> ReferenceSample<M> for f32 {
    fn to_reference_f64(self) -> f64 {
        f64::from(self)
    }
}
impl<M: ColorModel> ReferenceSample<M> for f64 {
    fn to_reference_f64(self) -> f64 {
        self
    }
}
impl<M: ColorModel> ReferenceSample<M> for UnitF32 {
    fn to_reference_f64(self) -> f64 {
        f64::from(self.get())
    }
}
impl<M: ColorModel> ReferenceSample<M> for UnitF64 {
    fn to_reference_f64(self) -> f64 {
        self.get()
    }
}
impl ReferenceSample<Rgb> for u8 {
    fn to_reference_f64(self) -> f64 {
        f64::from(self) / f64::from(u8::MAX)
    }
}
impl ReferenceSample<Rgb> for u16 {
    fn to_reference_f64(self) -> f64 {
        f64::from(self) / f64::from(u16::MAX)
    }
}

/// A device color model with a statically known number of channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DeviceN<const N: usize>;

impl<const N: usize> ColorModel for DeviceN<N> {
    type Coordinates<S: Sample> = [S; N];

    const COMPONENTS: usize = N;

    fn map_samples<S: Sample, T: Sample>(
        coordinates: &Self::Coordinates<S>,
        map: impl FnMut(S) -> T,
    ) -> Self::Coordinates<T> {
        coordinates.map(map)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_models_define_coordinate_arity_without_allocating() {
        let rgb: <Rgb as ColorModel>::Coordinates<u8> = [1, 2, 3];
        let cmyk: <Cmyk as ColorModel>::Coordinates<f32> = [0.1, 0.2, 0.3, 0.4];

        assert_eq!(rgb.as_slice(), &[1, 2, 3]);
        assert_eq!(cmyk.as_slice(), &[0.1, 0.2, 0.3, 0.4]);
        assert_eq!(<DeviceN<7> as ColorModel>::COMPONENTS, 7);
    }

    #[test]
    fn mapping_samples_preserves_model_and_channel_order() {
        assert_eq!(
            Rgb::map_samples(&[0_u8, 128, 255], ReferenceSample::<Rgb>::to_reference_f64),
            [0.0, 128.0 / 255.0, 1.0]
        );
        assert_eq!(
            DeviceN::<4>::map_samples(&[1_u8, 2, 3, 4], f64::from),
            [1.0, 2.0, 3.0, 4.0]
        );
    }
}
