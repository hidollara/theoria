use std::sync::Arc;

use skrifa::{FontRef, MetadataProvider};

use super::FontError;

/// Immutable font data and one selected face from that data.
pub(super) struct FontFace {
    data: Arc<[u8]>,
    face_index: u32,
}

impl FontFace {
    pub(super) fn from_bytes(data: impl Into<Arc<[u8]>>) -> Result<Self, FontError> {
        Self::from_collection_face(data, 0)
    }

    pub(super) fn from_collection_face(
        data: impl Into<Arc<[u8]>>,
        face_index: u32,
    ) -> Result<Self, FontError> {
        let data = data.into();
        FontRef::from_index(&data, face_index).map_err(|_| FontError::InvalidFace)?;
        Ok(Self { data, face_index })
    }

    pub(super) fn face_index(&self) -> u32 {
        self.face_index
    }

    pub(super) fn same_face(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data) && self.face_index == other.face_index
    }

    pub(super) fn skrifa_ref(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, self.face_index)
            .expect("FontFace validates immutable bytes when it is constructed")
    }

    pub(super) fn has_nominal_glyph(&self, character: char) -> bool {
        self.skrifa_ref()
            .charmap()
            .map(character)
            .is_some_and(|glyph| glyph.to_u32() != 0)
    }
}

impl std::fmt::Debug for FontFace {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FontFace")
            .field("byte_len", &self.data.len())
            .field("face_index", &self.face_index)
            .finish()
    }
}
