//! Lowers an ICC image transfer for the shared raster-sampling shader contract.

use crate::color::{ColorSpace, IccProfile, IccRenderingIntent, connect_spaces};

pub(crate) struct IccImageShader {
    pub(crate) source: String,
    pub(crate) tables: Vec<f32>,
}

/// `template` supplies the backend's vertex entry and sampling policy. Both
/// backends bind the profile's optional transfer tables at group 0, binding 2.
pub(crate) fn build(
    profile: &IccProfile,
    intent: IccRenderingIntent,
    has_alpha: bool,
    working_profile: impl Into<ColorSpace>,
    template: &str,
) -> Result<IccImageShader, Box<dyn std::error::Error>> {
    let transform = connect_spaces(
        profile,
        working_profile,
        intent,
        IccRenderingIntent::RelativeColorimetric,
    )?;
    let transfer = transform.transfer().wgsl_with_storage(0, 0, 2)?;
    let channels = profile.device_channels();
    let total = channels + usize::from(has_alpha);
    let layers = total.div_ceil(4);
    let components = ["r", "g", "b", "a"];
    let mut samples = String::new();
    for layer in 0..layers {
        samples.push_str(&format!(
            "    let layer_{layer} = textureSampleLevel(image_texture, image_sampler, uv, {layer}, 0.0);\n"
        ));
    }
    let values = (0..total)
        .map(|channel| format!("layer_{}.{}", channel / 4, components[channel % 4]))
        .collect::<Vec<_>>()
        .join(", ");
    samples.push_str(&format!("    return array<f32, {total}>({values});\n"));
    let input = (0..channels)
        .map(|channel| format!("sample[{channel}]"))
        .collect::<Vec<_>>()
        .join(", ");
    let alpha = if has_alpha {
        format!("sample[{channels}]")
    } else {
        "1.0".to_owned()
    };
    let source = template
        .replace("// @theoria-transfer", transfer.source())
        .replace("// @theoria-sample-values", &samples)
        .replace("THEORIA_SAMPLE_CHANNELS", &total.to_string())
        .replace("THEORIA_DEVICE_CHANNELS", &channels.to_string())
        .replace("THEORIA_DEVICE_VALUES", &input)
        .replace("THEORIA_ALPHA", &alpha);
    let tables = if transfer.table_values().is_empty() {
        vec![0.0f32]
    } else {
        transfer.table_values().to_vec()
    };
    Ok(IccImageShader { source, tables })
}
