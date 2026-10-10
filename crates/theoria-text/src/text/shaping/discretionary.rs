use super::{GlyphRun, virtual_text};

impl GlyphRun {
    /// Replaces one logical U+00AD with a visible hyphen for a line that
    /// actually breaks there, without creating a new source position.
    pub(in crate::text) fn with_visible_soft_hyphen(
        &self,
        content: &str,
        soft_hyphen: usize,
    ) -> Option<Self> {
        if !self.source_range.contains(&soft_hyphen)
            || content.get(soft_hyphen..)?.chars().next()? != '\u{00ad}'
        {
            return None;
        }
        let visible_hyphen = ['\u{2010}', '-']
            .into_iter()
            .find(|character| self.font.has_nominal_glyph(*character))?;
        let source = content.get(self.source_range.clone())?;
        let mut generated = String::new();
        let mut cluster_map = Vec::new();
        for (local, character) in source.char_indices() {
            let original = self.source_range.start + local;
            cluster_map.push((generated.len(), original));
            generated.push(if original == soft_hyphen {
                visible_hyphen
            } else {
                character
            });
        }
        virtual_text::reshape(self, &generated, &cluster_map)
    }

    /// Appends a visual hyphen at an existing logical boundary. The generated
    /// glyph joins the preceding logical cluster and does not invent a source
    /// position of its own.
    pub(in crate::text) fn with_inserted_hyphen(
        &self,
        content: &str,
        boundary: usize,
    ) -> Option<Self> {
        if boundary != self.source_range.end || !content.is_char_boundary(boundary) {
            return None;
        }
        let visible_hyphen = ['\u{2010}', '-']
            .into_iter()
            .find(|character| self.font.has_nominal_glyph(*character))?;
        let source = content.get(self.source_range.clone())?;
        let anchor = source
            .char_indices()
            .next_back()
            .map(|(local, _)| self.source_range.start + local)?;
        let mut generated = String::from(source);
        let mut cluster_map: Vec<_> = source
            .char_indices()
            .map(|(local, _)| (local, self.source_range.start + local))
            .collect();
        cluster_map.push((generated.len(), anchor));
        generated.push(visible_hyphen);
        virtual_text::reshape(self, &generated, &cluster_map)
    }

    /// Appends a visual ellipsis to a truncated line without extending the
    /// logical source range. The mark remains anchored to the final source
    /// cluster.
    pub(in crate::text) fn with_appended_ellipsis(&self, content: &str) -> Option<Self> {
        let ellipsis = ['\u{2026}', '\u{22ef}']
            .into_iter()
            .find(|character| self.font.has_nominal_glyph(*character))?;
        let source = content.get(self.source_range.clone())?;
        let anchor = source
            .char_indices()
            .next_back()
            .map(|(local, _)| self.source_range.start + local)?;
        let mut generated = String::from(source);
        let mut cluster_map: Vec<_> = source
            .char_indices()
            .map(|(local, _)| (local, self.source_range.start + local))
            .collect();
        cluster_map.push((generated.len(), anchor));
        generated.push(ellipsis);
        virtual_text::reshape(self, &generated, &cluster_map)
    }
}
