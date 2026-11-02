//! Editor-owned source. Parsing for inspection does not require a valid Organon
//! definition, and saving does not regenerate untouched text.
use organon_core::{Error, loader, syntax::Module};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    source: String,
}

impl Document {
    /// Keep incomplete or invalid input so a text editor can report and repair it.
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
        }
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn replace_source(&mut self, source: impl Into<String>) {
        self.source = source.into();
    }

    /// Inspect XML syntax, including attribute order, comments, and source ranges.
    /// Organon vocabulary and reference validity are not required here.
    pub fn xml(&self) -> Result<roxmltree::Document<'_>, Error> {
        roxmltree::Document::parse(&self.source).map_err(|e| Error(e.to_string()))
    }

    /// Extract shared definition content from the parsed tree without serializing it.
    pub fn definitions(&self) -> Result<Module, Error> {
        organon_core::xml::read(self.xml()?.root_element())
    }

    /// Replace an existing attribute value only. Path indexes count element
    /// children, starting at the root; surrounding formatting is untouched.
    pub fn replace_attribute(
        &mut self,
        path: &[usize],
        name: &str,
        value: &str,
    ) -> Result<(), Error> {
        let tree = self.xml()?;
        let mut node = tree.root_element();
        for &index in path {
            node = node
                .children()
                .filter(roxmltree::Node::is_element)
                .nth(index)
                .ok_or_else(|| Error("unknown element path".into()))?;
        }
        let attribute = node
            .attribute_node(name)
            .ok_or_else(|| Error(format!("missing attribute {name:?}")))?;
        let range = attribute.range_value();
        let quote = self.source.as_bytes()[range.start - 1];
        let escaped = crate::xml::escape_attribute(value);
        let escaped = if quote == b'\'' {
            escaped.replace('\'', "&apos;")
        } else {
            escaped
        };
        let mut source = self.source.clone();
        source.replace_range(range, &escaped);
        // Do not commit a GUI edit that would make the XML malformed.
        roxmltree::Document::parse(&source).map_err(|e| Error(e.to_string()))?;
        self.source = source;
        Ok(())
    }
}

/// Analyze unsaved content using the host's source identity and import resolver.
pub fn analyze<S: loader::Sources>(
    entry: S::Id,
    definitions: Module,
    dependencies: &mut S,
) -> Result<loader::Loaded<S::Id>, Error> {
    struct Edited<'a, S: loader::Sources> {
        entry: S::Id,
        definitions: Option<Module>,
        dependencies: &'a mut S,
    }
    impl<S: loader::Sources> loader::Sources for Edited<'_, S> {
        type Id = S::Id;
        fn resolve(
            &mut self,
            from: &Self::Id,
            import: &organon_core::syntax::ImportSource,
        ) -> Result<Self::Id, Error> {
            self.dependencies.resolve(from, import)
        }
        fn read(&mut self, id: &Self::Id) -> Result<Module, Error> {
            if id == &self.entry {
                self.definitions
                    .take()
                    .ok_or_else(|| Error("entry already read".into()))
            } else {
                self.dependencies.read(id)
            }
        }
    }
    let mut sources = Edited {
        entry: entry.clone(),
        definitions: Some(definitions),
        dependencies,
    };
    loader::load(entry, &mut sources)
}
