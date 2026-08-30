use std::sync::Arc;

use skrifa::{
    FontRef, MetadataProvider, Tag,
    instance::{Location, LocationRef, NormalizedCoord},
    setting::VariationSetting as SkrifaVariationSetting,
};

use super::{Font, FontFace, VariationSetting};

/// One face at a normalized variation location selected for execution.
#[derive(Clone)]
pub(crate) struct ResolvedFont {
    face: Arc<FontFace>,
    normalized_location: Location,
}

impl Font {
    pub(crate) fn resolve(&self) -> ResolvedFont {
        let face = self.face.skrifa_ref();
        ResolvedFont {
            face: Arc::clone(&self.face),
            normalized_location: normalized_location(&face, &self.variation_settings),
        }
    }
}

impl ResolvedFont {
    #[cfg(test)]
    pub(crate) fn same_instance(&self, other: &Self) -> bool {
        self.face.same_face(&other.face) && self.normalized_location == other.normalized_location
    }

    pub(crate) fn location(&self) -> LocationRef<'_> {
        (&self.normalized_location).into()
    }

    pub(crate) fn face(&self) -> FontRef<'_> {
        self.face.skrifa_ref()
    }

    pub(crate) fn has_nominal_glyph(&self, character: char) -> bool {
        self.face.has_nominal_glyph(character)
    }
}

impl std::fmt::Debug for ResolvedFont {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedFont")
            .field("face", &self.face)
            .field(
                "is_default_instance",
                &self.normalized_location.coords().is_empty(),
            )
            .finish()
    }
}

fn normalized_location(face: &FontRef<'_>, settings: &[VariationSetting]) -> Location {
    if settings.is_empty() {
        return Location::default();
    }

    let axes = face.axes();
    let skrifa_settings = settings
        .iter()
        .map(|setting| SkrifaVariationSetting::new(Tag::new(&setting.axis.bytes()), setting.value));
    let location = axes.location(skrifa_settings);

    if location
        .coords()
        .iter()
        .all(|coordinate| *coordinate == NormalizedCoord::default())
    {
        Location::default()
    } else {
        location
    }
}
