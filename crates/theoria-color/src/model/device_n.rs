use super::{ColorCoordinates, ColorModel};

/// A device color model with a statically known number of channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DeviceN<const N: usize>;

impl<const N: usize> ColorModel for DeviceN<N> {
    const COMPONENTS: usize = N;
}

impl<const N: usize> ColorCoordinates<DeviceN<N>> for [f32; N] {}
impl<const N: usize> ColorCoordinates<DeviceN<N>> for [f64; N] {}
