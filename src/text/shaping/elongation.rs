use super::{GlyphRun, virtual_text};

impl GlyphRun {
    pub(in crate::text) fn tatweel_insertion_positions(&self) -> Vec<usize> {
        let mut positions = Vec::new();
        for glyph in self
            .glyphs
            .iter()
            .filter(|glyph| glyph.safe_to_insert_tatweel)
        {
            if positions.last() != Some(&glyph.cluster) {
                positions.push(glyph.cluster);
            }
        }
        positions
    }

    /// Re-shapes this run after inserting virtual U+0640 characters. Generated
    /// clusters are mapped back to the original text so layout does not invent
    /// logical source positions.
    pub(in crate::text) fn with_tatweels(
        &self,
        content: &str,
        insertions: &[usize],
    ) -> Option<Self> {
        if insertions.is_empty() {
            return Some(self.clone());
        }
        let source = content.get(self.source_range.clone())?;
        let mut insertions = insertions.to_vec();
        insertions.sort_unstable();
        if insertions.iter().any(|position| {
            *position < self.source_range.start
                || *position >= self.source_range.end
                || !content.is_char_boundary(*position)
        }) {
            return None;
        }

        let mut virtual_text = String::new();
        let mut cluster_map = Vec::new();
        let mut insertion_index = 0;
        for (local, character) in source.char_indices() {
            let original = self.source_range.start + local;
            while insertions.get(insertion_index) == Some(&original) {
                cluster_map.push((virtual_text.len(), original));
                virtual_text.push('\u{0640}');
                insertion_index += 1;
            }
            cluster_map.push((virtual_text.len(), original));
            virtual_text.push(character);
        }
        if insertion_index != insertions.len() {
            return None;
        }

        virtual_text::reshape(self, &virtual_text, &cluster_map)
    }
}
