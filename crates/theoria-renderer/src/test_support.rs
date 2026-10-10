use theoria_color::{ColorProfile, ColorSpace, ConnectionSpace, SpaceConnectionError, Transfer};

/// A separate runtime space with the same conversions and coordinate geometry.
/// This exercises space identity without manufacturing another standard space.
pub(crate) fn distinct_color_space(space: impl Into<ColorSpace>) -> ColorSpace {
    struct Distinct(ColorSpace, u64);

    impl ColorProfile for Distinct {
        fn equivalent(&self, other: &dyn ColorProfile) -> bool {
            (other as &dyn std::any::Any)
                .downcast_ref::<Self>()
                .is_some_and(|other| self.1 == other.1)
        }

        fn hash(&self, mut state: &mut dyn std::hash::Hasher) {
            std::hash::Hash::hash(&self.1, &mut state);
        }

        fn model(&self) -> theoria_color::ColorModel {
            self.0.model()
        }

        fn connection_space(&self) -> ConnectionSpace {
            self.0.connection_space()
        }

        fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
            self.0.transfer_to_connection_space()
        }

        fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
            self.0.transfer_from_connection_space()
        }

        fn direct_transfer_to(
            &self,
            destination: &ColorSpace,
        ) -> Result<Option<Transfer>, SpaceConnectionError> {
            self.0.direct_to(destination)
        }
    }

    let space = space.into();
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    ColorSpace::new(Distinct(
        space,
        NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
    ))
}
