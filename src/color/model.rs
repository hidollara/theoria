use super::Sample;

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
}

macro_rules! fixed_model {
    ($name:ident, $components:expr) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl ColorModel for $name {
            type Coordinates<S: Sample> = [S; $components];

            const COMPONENTS: usize = $components;
        }
    };
}

fixed_model!(Rgb, 3);
fixed_model!(Cmyk, 4);
fixed_model!(Xyz, 3);
fixed_model!(Lab, 3);
fixed_model!(Ycbcr, 3);

/// A device color model with a statically known number of channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DeviceN<const N: usize>;

impl<const N: usize> ColorModel for DeviceN<N> {
    type Coordinates<S: Sample> = [S; N];

    const COMPONENTS: usize = N;
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
}
