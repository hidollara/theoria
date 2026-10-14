//! Dynamic evaluation for preview. Inputs are host values, not source code.
//! Persistent bindings share the same value/caching policy as generated Rust.
mod source;
use crate::{CanvasExpr, Element, element::ValueExpr};
use organon_format::{CoverageExpr, InputType, PathExpr, Segment, Template};
pub use source::{Read as SourceRead, Value as Source};
use std::{collections::BTreeMap, error::Error as StdError, fmt};
use theoria_canvas::{
    Canvas, Color, ColorField, Composite, Composition, Coverage, Opacity,
    color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb},
    geometry::{Offset, Point},
    path::Path,
    transform::Affine2,
};
use theoria_text::{
    font::Font,
    text::{Span, Text, TextStyle},
};

#[derive(Clone, Debug)]
pub enum Value {
    Scalar(f32),
    String(String),
    Color(Color),
    ColorField(ColorField),
    Canvas(Canvas),
    Path(Path),
    Fonts(Vec<Font>),
}

impl Value {
    pub fn kind(&self) -> InputType {
        match self {
            Self::Scalar(_) => InputType::Scalar,
            Self::String(_) => InputType::String,
            Self::Color(_) => InputType::Color,
            Self::ColorField(_) => InputType::ColorField,
            Self::Canvas(_) => InputType::Canvas,
            Self::Path(_) => InputType::Path,
            Self::Fonts(_) => InputType::Fonts,
        }
    }
}

pub type Values = BTreeMap<String, Value>;
type BorrowedValues<'a> = BTreeMap<String, &'a Value>;

/// Inputs or exports that can be fixed, reactive, or inline expressions.
pub type Bindings = BTreeMap<String, Source<Result<Value>>>;

/// Reads CLI/sample text. Native resources must be supplied through `Values`.
pub fn sample_value(kind: InputType, text: &str) -> Result<Value> {
    match kind {
        InputType::String => Ok(Value::String(text.into())),
        InputType::Scalar => {
            let value: f32 = text
                .parse()
                .map_err(|_| Error("sample must be a scalar".into()))?;
            if !value.is_finite() {
                return Err(Error("sample must be finite".into()));
            }
            Ok(Value::Scalar(value))
        }
        InputType::Color => color(text, &BorrowedValues::new()).map(Value::Color),
        _ => Err(Error(format!(
            "{kind:?} samples must be supplied as native Rust values"
        ))),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl StdError for Error {}
type Result<T> = std::result::Result<T, Error>;

fn requires_compilation(value: &organon_format::ValueExpr) -> bool {
    fn canvas(value: &organon_format::CanvasExpr) -> bool {
        use organon_format::CanvasExpr;
        match value {
            CanvasExpr::Get(_) => true,
            CanvasExpr::Draw {
                coverage: CoverageExpr::Fill(path),
                ..
            } => matches!(path, PathExpr::Get(_)),
            CanvasExpr::Translate { content, .. } => canvas(content),
            CanvasExpr::SourceOver {
                backdrop, source, ..
            } => canvas(backdrop) || canvas(source),
            CanvasExpr::Draw { .. } | CanvasExpr::Text { .. } | CanvasExpr::Reference(_) => false,
        }
    }
    match value {
        organon_format::ValueExpr::Instance(_) | organon_format::ValueExpr::Get(_) => true,
        organon_format::ValueExpr::Canvas(value) => canvas(value),
        organon_format::ValueExpr::Path(value) => matches!(value, PathExpr::Get(_)),
        organon_format::ValueExpr::Reference(_) => false,
    }
}

impl Element {
    /// Re-evaluates current authored data with one set of sample or real inputs.
    /// Outputs retain their declared coordinates; presentation transforms belong
    /// to the host and must also be applied to its hit regions when appropriate.
    pub fn evaluate(&self, inputs: &Values) -> Result<Values> {
        let inputs = inputs
            .iter()
            .map(|(name, value)| (name.clone(), Source::fixed(Ok(value.clone()))))
            .collect();
        self.bind(&theoria_reactive::Reactive::new(), &inputs)?
            .into_iter()
            .map(|(name, value)| {
                let value = value
                    .as_fixed()
                    .cloned()
                    .expect("fixed inputs produce fixed outputs")?;
                Ok((name, value))
            })
            .collect()
    }

    /// Construct once, then read the returned exports inside host Computeds.
    /// All reactive inputs and readers must belong to `reactive`. Editing the
    /// definition requires rebinding; changing an input only invalidates its
    /// dependents. Inline bindings execute in their reader's scope.
    pub fn bind(
        &self,
        reactive: &theoria_reactive::Reactive,
        inputs: &Bindings,
    ) -> Result<Bindings> {
        let schema = organon_format::ElementDefinition::from(self.clone());
        if schema
            .definitions
            .iter()
            .any(|d| requires_compilation(&d.value))
            || schema
                .inputs
                .iter()
                .any(|i| matches!(i.kind, InputType::Element(_)))
        {
            return Err(Error(
                "component instances and Element-valued inputs require compilation; preview registration is not implemented"
                    .into(),
            ));
        }
        let definitions = schema
            .ordered_bindings()
            .map_err(|e| Error(e.to_string()))?;
        let mut bindings = Bindings::new();
        for input in &self.inputs {
            let source = inputs
                .get(&input.name)
                .ok_or_else(|| Error(format!("missing input {:?}", input.name)))?
                .clone();
            let input = input.clone();
            let name = input.name.clone();
            let check = move |value: Result<Value>| {
                let value = value?;
                if value.kind() != input.kind {
                    return Err(Error(format!(
                        "input {:?}: expected {:?}, got {:?}",
                        input.name,
                        input.kind,
                        value.kind()
                    )));
                }
                Ok(value)
            };
            let checked = if let Some(value) = source.as_fixed() {
                Source::fixed(Ok(check(value.clone())?))
            } else {
                Source::inline(move |cx| check(source.read(cx).into_cloned()))
            };
            bindings.insert(name, checked);
        }
        for name in inputs.keys() {
            if !bindings.contains_key(name) {
                return Err(Error(format!("undeclared input {name:?}")));
            }
        }
        let mut outputs = Bindings::new();
        for definition in definitions {
            let sources: Bindings = definition
                .references()
                .map_err(|e| Error(e.to_string()))?
                .into_iter()
                .map(|name| {
                    let value = bindings
                        .get(&name)
                        .ok_or_else(|| Error(format!("unknown reference {name:?}")))?
                        .clone();
                    Ok((name, value))
                })
                .collect::<Result<_>>()?;
            let (names, sources): (Vec<_>, Vec<_>) = sources.into_iter().unzip();
            let name = definition.name.clone();
            let export = definition.export;
            let inline = definition.inline;
            let definition = crate::element::Let::from(definition);
            let fixed: Option<Vec<_>> = sources.iter().map(|value| value.as_fixed()).collect();
            let evaluate = move |values: Vec<&Result<Value>>| {
                let values: BorrowedValues<'_> = names
                    .iter()
                    .cloned()
                    .zip(values)
                    .map(|(name, value)| Ok((name, value.as_ref().map_err(Clone::clone)?)))
                    .collect::<Result<_>>()?;
                let value = match &definition.value {
                    ValueExpr::Get(_) => Err(Error(
                        "get requires compilation; preview registration is not implemented".into(),
                    )),
                    ValueExpr::Canvas(value) => value.evaluate(&values).map(Value::Canvas),
                    ValueExpr::Path(value) => evaluate_path(value, &values).map(Value::Path),
                    ValueExpr::Reference(reference) => resource(reference, &values).cloned(),
                    ValueExpr::Instance(_) => unreachable!("instances rejected before evaluation"),
                }
                .map_err(|e| Error(format!("binding {:?}: {e}", definition.name)))?;
                if let Value::Path(path) = &value
                    && !path.is_finite()
                {
                    return Err(Error("Path coordinates must be finite".into()));
                }
                Ok(value)
            };
            let value = if let Some(values) = fixed {
                Source::fixed(evaluate(values))
            } else {
                let read = move |cx: &theoria_reactive::ReadContext<'_>| {
                    let reads: Vec<_> = sources.iter().map(|source| source.read(cx)).collect();
                    evaluate(reads.iter().map(|read| &**read).collect())
                };
                if inline {
                    Source::inline(read)
                } else {
                    Source::reactive(reactive.create_computed(move |cx| read(cx)).value())
                }
            };
            if export {
                outputs.insert(name.clone(), value.clone());
            }
            bindings.insert(name, value);
        }
        Ok(outputs)
    }
}

fn parse(value: &str) -> Result<Template> {
    Template::parse(value).map_err(|e| Error(e.to_string()))
}

fn expand(template: &Template, inputs: &BorrowedValues<'_>) -> Result<String> {
    let mut text = String::new();
    for segment in template.segments() {
        match segment {
            Segment::Literal(value) => text.push_str(value),
            Segment::Reference(name) => match inputs.get(name) {
                Some(Value::String(value)) => text.push_str(value),
                Some(Value::Scalar(value)) => text.push_str(&value.to_string()),
                Some(_) => return Err(Error(format!("{name:?} is not a textual value"))),
                None => return Err(Error(format!("unknown reference {name:?}"))),
            },
        }
    }
    Ok(text)
}

fn resource<'a>(value: &str, inputs: &'a BorrowedValues<'_>) -> Result<&'a Value> {
    let template = parse(value)?;
    let name = template
        .reference()
        .ok_or_else(|| Error("expected a single {name} reference".into()))?;
    inputs
        .get(name)
        .copied()
        .ok_or_else(|| Error(format!("unknown reference {name:?}")))
}

fn scalar(value: &str, inputs: &BorrowedValues<'_>) -> Result<f32> {
    let template = parse(value)?;
    let value = if let Some(Value::Scalar(value)) = template.reference().and_then(|n| inputs.get(n))
    {
        *value
    } else {
        expand(&template, inputs)?
            .parse()
            .map_err(|_| Error("attribute did not expand to a scalar".into()))?
    };
    if !value.is_finite() {
        return Err(Error("scalar must be finite".into()));
    }
    Ok(value)
}

fn color(value: &str, inputs: &BorrowedValues<'_>) -> Result<Color> {
    let template = parse(value)?;
    if let Some(Value::Color(color)) = template.reference().and_then(|n| inputs.get(n)) {
        return Ok(color.clone());
    }
    let value = expand(&template, inputs)?;
    let hex = value
        .strip_prefix('#')
        .filter(|s| matches!(s.len(), 6 | 8) && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| Error("color must expand to #RRGGBB or #RRGGBBAA".into()))?;
    let mut rgba = [255_u8; 4];
    for (index, channel) in rgba.iter_mut().take(hex.len() / 2).enumerate() {
        *channel = u8::from_str_radix(&hex[2 * index..2 * index + 2], 16)
            .map_err(|e| Error(e.to_string()))?;
    }
    Ok(Color::new(
        Srgb,
        [rgba[0], rgba[1], rgba[2]],
        Opacity::<u8>::from(rgba[3]),
    ))
}

fn path(value: &str, inputs: &BorrowedValues<'_>) -> Result<Path> {
    match resource(value, inputs)? {
        Value::Path(path) if path.is_finite() => Ok(path.clone()),
        _ => Err(Error("expected a finite Path".into())),
    }
}

impl CanvasExpr {
    fn evaluate(&self, inputs: &BorrowedValues<'_>) -> Result<Canvas> {
        Ok(match self {
            Self::Get(_) => {
                return Err(Error(
                    "get requires compilation; preview registration is not implemented".into(),
                ));
            }
            Self::Draw { source, coverage } => Canvas::draw(
                color_field(source, inputs)?,
                match coverage {
                    CoverageExpr::All => Coverage::all(),
                    CoverageExpr::Fill(path) => Coverage::fill(evaluate_path(path, inputs)?),
                },
            ),
            Self::Reference(reference) => match resource(reference, inputs)? {
                Value::Canvas(canvas) => canvas.clone(),
                _ => return Err(Error("ref in this position requires a Canvas".into())),
            },
            Self::Translate { x, y, content } => content.evaluate(inputs)?.transformed(
                Affine2::from_translation(Offset::new(scalar(x, inputs)?, scalar(y, inputs)?))
                    .ok_or_else(|| Error("invalid translation".into()))?,
            ),
            Self::SourceOver {
                space,
                backdrop,
                source,
            } => {
                let space: theoria_canvas::color::ColorSpace = match space.as_str() {
                    "Srgb" => Srgb.into(),
                    "LinearSrgb" => LinearSrgb.into(),
                    "DisplayP3" => DisplayP3.into(),
                    "LinearDisplayP3" => LinearDisplayP3.into(),
                    _ => return Err(Error("unsupported composition space".into())),
                };
                backdrop.evaluate(inputs)?.composite(
                    source.evaluate(inputs)?,
                    Composition::new(Composite::SourceOver, space),
                )
            }
            Self::Text {
                content,
                style,
                wrap_width,
            } => {
                let Value::Fonts(fonts) = resource(&style.fonts, inputs)? else {
                    return Err(Error("fonts requires a font chain".into()));
                };
                let foreground = color_field(&style.foreground, inputs)?;
                let parent = TextStyle::new(fonts, scalar(&style.font_size, inputs)?)
                    .with_foreground(foreground.clone());
                let mut spans = Vec::new();
                text_spans(content, &parent, inputs, &mut spans)?;
                let mut text = Text::horizontal(parent).extend(spans);
                if let Some(width) = wrap_width {
                    text = text.wrap_width(scalar(width, inputs)?);
                }
                text.layout()
                    .map_err(|e| Error(e.to_string()))?
                    .to_canvas(foreground)
                    .map_err(|e| Error(e.to_string()))?
                    .unwrap_or_else(|| {
                        Canvas::draw(
                            Color::new(Srgb, [0_u8; 3], Opacity::<u8>::ZERO),
                            Coverage::all(),
                        )
                    })
            }
        })
    }
}

fn color_field(value: &str, inputs: &BorrowedValues<'_>) -> Result<ColorField> {
    let template = parse(value)?;
    if let Some(value) = template.reference().and_then(|name| inputs.get(name)) {
        match value {
            Value::ColorField(field) => return Ok(field.clone()),
            Value::Canvas(canvas) => return Ok(canvas.clone().into()),
            _ => (),
        }
    }
    color(value, inputs).map(Into::into)
}

fn evaluate_path(expr: &PathExpr, inputs: &BorrowedValues<'_>) -> Result<Path> {
    match expr {
        PathExpr::Get(_) => Err(Error(
            "get requires compilation; preview registration is not implemented".into(),
        )),
        PathExpr::Reference(reference) => path(reference, inputs),
        PathExpr::Polygon(points) => {
            let points = points
                .iter()
                .map(|p| Ok(Point::new(scalar(&p.x, inputs)?, scalar(&p.y, inputs)?)))
                .collect::<Result<Vec<_>>>()?;
            Ok(Path::polygon(points))
        }
        PathExpr::Rect {
            x,
            y,
            width,
            height,
        } => {
            let (x, y, width, height) = (
                scalar(x, inputs)?,
                scalar(y, inputs)?,
                scalar(width, inputs)?,
                scalar(height, inputs)?,
            );
            let (right, bottom) = (x + width, y + height);
            if width < 0.0 || height < 0.0 || !right.is_finite() || !bottom.is_finite() {
                return Err(Error(
                    "Rect requires finite coordinates and nonnegative dimensions".into(),
                ));
            }
            Ok(Path::polygon([
                Point::new(x, y),
                Point::new(right, y),
                Point::new(right, bottom),
                Point::new(x, bottom),
            ]))
        }
    }
}

fn override_text_style(
    style: &organon_format::TextStyleOverride,
    parent: &TextStyle,
    inputs: &BorrowedValues<'_>,
) -> Result<TextStyle> {
    let mut resolved = parent.clone();
    if let Some(fonts) = &style.fonts {
        let Value::Fonts(fonts) = resource(fonts, inputs)? else {
            return Err(Error("fonts requires a font chain".into()));
        };
        resolved = resolved.with_fonts(fonts);
    }
    if let Some(size) = &style.font_size {
        let size = scalar(size, inputs)?;
        if size < 0.0 {
            return Err(Error("font size must be nonnegative".into()));
        }
        resolved = resolved.with_font_size(size);
    }
    if let Some(foreground) = &style.foreground {
        resolved = resolved.with_foreground(color_field(foreground, inputs)?);
    }
    Ok(resolved)
}

fn text_spans(
    nodes: &[organon_format::TextContent],
    parent: &TextStyle,
    inputs: &BorrowedValues<'_>,
    output: &mut Vec<Span>,
) -> Result<()> {
    use organon_format::TextContent;
    for node in nodes {
        match node {
            TextContent::String { template, style } => {
                output.push(
                    Span::new(expand(&parse(template)?, inputs)?)
                        .with_style(override_text_style(style, parent, inputs)?),
                );
            }
            TextContent::Break => output.push(Span::new("\n").with_style(parent.clone())),
            TextContent::Span { style, children } => {
                let style = override_text_style(style, parent, inputs)?;
                text_spans(children, &style, inputs, output)?;
            }
        }
    }
    Ok(())
}
