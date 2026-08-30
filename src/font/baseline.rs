use skrifa::{
    FontRef, Tag,
    instance::LocationRef,
    raw::{
        TableProvider,
        tables::{
            base::{Base, BaseCoord, BaseValues},
            layout::DeviceOrVariationIndex,
            variations::DeltaSetIndex,
        },
    },
};

/// Selects the BASE table associated with a physical text direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BaselineAxis {
    Horizontal,
    Vertical,
}

/// Baseline identities currently needed by Theoria's layout defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BaselineKind {
    Alphabetic,
    IdeographicCenter,
}

const ROMAN: Tag = Tag::new(b"romn");
const IDEOGRAPHIC_LEFT: Tag = Tag::new(b"ideo");
const IDEOGRAPHIC_RIGHT: Tag = Tag::new(b"idtp");
const DEFAULT_SCRIPT: Tag = Tag::new(b"DFLT");

/// Resolves a semantic baseline from one OpenType BASE axis.
pub(super) fn coordinate(
    face: FontRef<'_>,
    location: LocationRef<'_>,
    axis_kind: BaselineAxis,
    script: Option<[u8; 4]>,
    baseline: BaselineKind,
) -> Option<f32> {
    let base = face.base().ok()?;
    let axis = match axis_kind {
        BaselineAxis::Horizontal => base.horiz_axis()?.ok()?,
        BaselineAxis::Vertical => base.vert_axis()?.ok()?,
    };
    let tags = axis.base_tag_list()?.ok()?.baseline_tags();

    let scripts = axis.base_script_list().ok()?;
    let requested = script.map(open_type_script_tag);
    let record = requested
        .and_then(|tag| {
            scripts
                .base_script_records()
                .iter()
                .find(|record| record.base_script_tag() == tag)
        })
        .or_else(|| {
            scripts
                .base_script_records()
                .iter()
                .find(|record| record.base_script_tag() == DEFAULT_SCRIPT)
        })?;
    let values = record
        .base_script(scripts.offset_data())
        .ok()?
        .base_values()?
        .ok()?;

    match baseline {
        BaselineKind::Alphabetic => {
            table_coordinate(&base, &values, tag_index(tags, ROMAN)?, location)
        }
        BaselineKind::IdeographicCenter => {
            let units_per_em = f32::from(face.head().ok()?.units_per_em());
            let bottom_or_left = tag_index(tags, IDEOGRAPHIC_LEFT)
                .and_then(|index| table_coordinate(&base, &values, index, location));
            let top_or_right = tag_index(tags, IDEOGRAPHIC_RIGHT)
                .and_then(|index| table_coordinate(&base, &values, index, location));
            let (start, end) = match (axis_kind, bottom_or_left, top_or_right) {
                (BaselineAxis::Horizontal, Some(start), Some(end)) => (start, end),
                (BaselineAxis::Horizontal, Some(start), None) => (start, start + units_per_em),
                (BaselineAxis::Horizontal, None, _) => return None,
                (BaselineAxis::Vertical, start, end) => {
                    (start.unwrap_or(0.0), end.unwrap_or(units_per_em))
                }
            };
            Some((start + end) * 0.5)
        }
    }
}

fn tag_index(tags: &[skrifa::raw::types::BigEndian<Tag>], target: Tag) -> Option<usize> {
    tags.iter().position(|tag| tag.get() == target)
}

fn table_coordinate(
    base: &Base<'_>,
    values: &BaseValues<'_>,
    index: usize,
    location: LocationRef<'_>,
) -> Option<f32> {
    let coordinate = values.base_coords().get(index).ok()?;
    let mut value = f32::from(coordinate.coordinate());
    if let BaseCoord::Format3(format3) = coordinate
        && let Some(Ok(DeviceOrVariationIndex::VariationIndex(index))) = format3.device()
        && let Some(Ok(store)) = base.item_var_store()
    {
        let delta = store
            .compute_delta(
                DeltaSetIndex {
                    outer: index.delta_set_outer_index(),
                    inner: index.delta_set_inner_index(),
                },
                location.coords(),
            )
            .ok()?;
        value += delta as f32;
    }
    Some(value)
}

/// Converts the ISO 15924 identity used by text itemization to the OpenType
/// layout-script identity used by BASE records.
const fn open_type_script_tag(script: [u8; 4]) -> Tag {
    match &script {
        b"Hira" | b"Hrkt" => Tag::new(b"kana"),
        b"Laoo" => Tag::new(b"lao "),
        b"Yiii" => Tag::new(b"yi  "),
        b"Nkoo" => Tag::new(b"nko "),
        b"Vaii" => Tag::new(b"vai "),
        _ => Tag::new(&[
            script[0].to_ascii_lowercase(),
            script[1],
            script[2],
            script[3],
        ]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_scripts_map_to_open_type_layout_tags() {
        assert_eq!(open_type_script_tag(*b"Latn"), Tag::new(b"latn"));
        assert_eq!(open_type_script_tag(*b"Hani"), Tag::new(b"hani"));
        assert_eq!(open_type_script_tag(*b"Hira"), Tag::new(b"kana"));
        assert_eq!(open_type_script_tag(*b"Laoo"), Tag::new(b"lao "));
    }
}
