//! Lift anonymous constructions and selections into typed, named expressions.
//! A ref always names one binding; get always selects one output.
use crate::Error;
use organon_format::{CanvasExpr, CoverageExpr, ElementDefinition, Get, Let, PathExpr, ValueExpr};
use std::collections::HashSet;

pub(crate) fn normalize(definition: &ElementDefinition) -> Result<ElementDefinition, Error> {
    definition
        .ordered_bindings()
        .map_err(|e| Error(e.to_string()))?;
    let mut definition = definition.clone();
    let mut selection = Selection {
        used: definition
            .inputs
            .iter()
            .map(|i| i.name.clone())
            .chain(definition.definitions.iter().map(|d| d.name.clone()))
            .collect(),
        lifted: Vec::new(),
    };
    for binding in &mut definition.definitions {
        selection.value(&mut binding.value)?;
    }
    definition.definitions.extend(selection.lifted);
    Ok(definition)
}

struct Selection {
    used: HashSet<String>,
    lifted: Vec<Let>,
}
impl Selection {
    fn lift(&mut self, mut value: ValueExpr) -> Result<String, Error> {
        self.value(&mut value)?;
        let mut name = format!("__organon_expression_{}", self.used.len());
        while !self.used.insert(name.clone()) {
            name.push('_');
        }
        self.lifted.push(Let {
            name: name.clone(),
            export: false,
            inline: true,
            value,
        });
        Ok(format!("{{{name}}}"))
    }
    fn get(&mut self, get: &mut Get) -> Result<(), Error> {
        if !matches!(*get.target, ValueExpr::Reference(_)) {
            let reference = self.lift(*get.target.clone())?;
            *get.target = ValueExpr::Reference(reference);
        }
        Ok(())
    }
    fn value(&mut self, value: &mut ValueExpr) -> Result<(), Error> {
        match value {
            ValueExpr::Get(get) => self.get(get)?,
            ValueExpr::Canvas(canvas) => self.canvas(canvas)?,
            ValueExpr::Path(path) => self.path(path)?,
            ValueExpr::Reference(_) | ValueExpr::Instance(_) => (),
        }
        Ok(())
    }
    fn canvas(&mut self, value: &mut CanvasExpr) -> Result<(), Error> {
        match value {
            CanvasExpr::Get(get) => {
                *value = CanvasExpr::Reference(self.lift(ValueExpr::Get(get.clone()))?)
            }
            CanvasExpr::Draw {
                coverage: CoverageExpr::Fill(path),
                ..
            } => self.path(path)?,
            CanvasExpr::Translate { content, .. } => self.canvas(content)?,
            CanvasExpr::SourceOver {
                backdrop, source, ..
            } => {
                self.canvas(backdrop)?;
                self.canvas(source)?;
            }
            CanvasExpr::Draw { .. } | CanvasExpr::Text { .. } | CanvasExpr::Reference(_) => (),
        }
        Ok(())
    }
    fn path(&mut self, value: &mut PathExpr) -> Result<(), Error> {
        if let PathExpr::Get(get) = value {
            *value = PathExpr::Reference(self.lift(ValueExpr::Get(get.clone()))?);
        }
        Ok(())
    }
}
