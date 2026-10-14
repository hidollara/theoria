//! A structural view of bindings for consumers of the format. Authored trees
//! remain intact; neither Rust generation nor native value evaluation lives here.
use crate::{
    CanvasExpr, CoverageExpr, ElementDefinition, Error, Let, PathExpr, Segment, Template, ValueExpr,
};
use std::collections::{HashMap, VecDeque};

impl ElementDefinition {
    /// Returns top-level bindings in dependency order. Unknown references and
    /// cycles are errors, including in private definitions. Serialization
    /// retains the authored declaration order.
    pub fn ordered_bindings(&self) -> Result<Vec<Let>, Error> {
        self.validate()?;
        let definitions = &self.definitions;
        let indices: HashMap<_, _> = definitions
            .iter()
            .enumerate()
            .map(|(i, d)| (d.name.as_str(), i))
            .collect();
        let mut pending = vec![0; definitions.len()];
        let mut dependents = vec![Vec::new(); definitions.len()];
        for (index, definition) in definitions.iter().enumerate() {
            for name in references(&definition.value)? {
                let root = name.split('.').next().unwrap();
                if let Some(&dependency) = indices.get(root) {
                    pending[index] += 1;
                    dependents[dependency].push(index);
                } else if !self.inputs.iter().any(|input| input.name == root) {
                    return Err(Error(format!(
                        "binding {:?}: unknown reference {name:?}",
                        definition.name
                    )));
                }
            }
        }
        let mut ready: VecDeque<_> = pending
            .iter()
            .enumerate()
            .filter_map(|(i, &count)| (count == 0).then_some(i))
            .collect();
        let mut ordered = Vec::new();
        while let Some(index) = ready.pop_front() {
            ordered.push(definitions[index].clone());
            for &dependent in &dependents[index] {
                pending[dependent] -= 1;
                if pending[dependent] == 0 {
                    ready.push_back(dependent);
                }
            }
        }
        if ordered.len() != definitions.len() {
            let names: Vec<_> = pending
                .iter()
                .enumerate()
                .filter_map(|(i, &count)| (count > 0).then_some(definitions[i].name.as_str()))
                .collect();
            return Err(Error(format!(
                "cyclic binding dependencies: {}",
                names.join(", ")
            )));
        }
        Ok(ordered)
    }
}

impl Let {
    /// Names read by this expression, sorted and deduplicated.
    pub fn references(&self) -> Result<Vec<String>, Error> {
        references(&self.value)
    }
}

fn references(value: &ValueExpr) -> Result<Vec<String>, Error> {
    let mut attributes = Vec::new();
    value_attributes(value, &mut attributes);
    let mut names = Vec::new();
    for attribute in attributes {
        for segment in Template::parse(attribute)?.segments() {
            if let Segment::Reference(name) = segment {
                names.push(name.clone());
            }
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn value_attributes<'a>(value: &'a ValueExpr, attributes: &mut Vec<&'a str>) {
    match value {
        ValueExpr::Get(value) => value_attributes(&value.target, attributes),
        ValueExpr::Reference(value) => attributes.push(value.as_str()),
        ValueExpr::Canvas(value) => canvas_attributes(value, attributes),
        ValueExpr::Path(value) => path_attributes(value, attributes),
        ValueExpr::Instance(value) => {
            attributes.extend(value.inputs.iter().map(|(_, v)| v.as_str()))
        }
    }
}

fn canvas_attributes<'a>(node: &'a CanvasExpr, attributes: &mut Vec<&'a str>) {
    match node {
        CanvasExpr::Get(value) => value_attributes(&value.target, attributes),
        CanvasExpr::Draw { source, coverage } => {
            attributes.push(source);
            if let CoverageExpr::Fill(path) = coverage {
                path_attributes(path, attributes);
            }
        }
        CanvasExpr::Text {
            content,
            style,
            wrap_width,
        } => {
            for child in content {
                child.attributes(attributes);
            }
            attributes.extend([style.fonts.as_str(), &style.font_size, &style.foreground]);
            if let Some(width) = wrap_width {
                attributes.push(width);
            }
        }
        CanvasExpr::Translate { x, y, content } => {
            attributes.extend([x.as_str(), y]);
            canvas_attributes(content, attributes);
        }
        CanvasExpr::SourceOver {
            backdrop, source, ..
        } => {
            canvas_attributes(backdrop, attributes);
            canvas_attributes(source, attributes);
        }
        CanvasExpr::Reference(value) => attributes.push(value),
    }
}

fn path_attributes<'a>(node: &'a PathExpr, attributes: &mut Vec<&'a str>) {
    match node {
        PathExpr::Get(value) => value_attributes(&value.target, attributes),
        PathExpr::Reference(value) => attributes.push(value),
        PathExpr::Rect {
            x,
            y,
            width,
            height,
        } => attributes.extend([x.as_str(), y, width, height]),
        PathExpr::Polygon(points) => {
            for point in points {
                attributes.extend([point.x.as_str(), &point.y]);
            }
        }
    }
}
