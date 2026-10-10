use super::{OpenTypeFeatureTag, ShapingOptions};
use crate::text::TextStyle;

impl TextStyle {
    pub(in crate::text) fn shaping_options(&self, context: &ShapingOptions) -> ShapingOptions {
        let mut options = context.clone();
        if let Some(language) = self.language() {
            options.language = Some(language.to_owned());
        }
        options.features.extend(self.features().iter().copied());
        if self.letter_spacing() != 0.0 {
            for tag in [
                OpenTypeFeatureTag::STANDARD_LIGATURES,
                OpenTypeFeatureTag::CONTEXTUAL_LIGATURES,
            ] {
                if !options.features.iter().any(|feature| feature.tag() == tag) {
                    options.features.push(tag.disabled());
                }
            }
        }
        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracking_disables_default_ligatures_unless_explicitly_overridden() {
        let style = TextStyle::new(&[], 16.0)
            .with_letter_spacing(2.0)
            .with_feature(OpenTypeFeatureTag::STANDARD_LIGATURES.enabled());
        let options = style.shaping_options(&ShapingOptions::default());

        assert!(
            options
                .features
                .contains(&OpenTypeFeatureTag::STANDARD_LIGATURES.enabled())
        );
        assert!(
            !options
                .features
                .contains(&OpenTypeFeatureTag::STANDARD_LIGATURES.disabled())
        );
        assert!(
            options
                .features
                .contains(&OpenTypeFeatureTag::CONTEXTUAL_LIGATURES.disabled())
        );
    }
}
