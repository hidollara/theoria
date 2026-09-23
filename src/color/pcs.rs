/// ICC D50 PCS white, expressed as actual XYZ tristimulus values.
const ICC_D50: [f64; 3] = [0.9642, 1.0, 0.8249];
const DELTA: f64 = 6.0 / 29.0;
const EPSILON: f64 = DELTA * DELTA * DELTA;

fn lab_f(value: f64) -> f64 {
    if value > EPSILON {
        value.cbrt()
    } else {
        value / (3.0 * DELTA * DELTA) + 4.0 / 29.0
    }
}

fn lab_f_inverse(value: f64) -> f64 {
    if value > DELTA {
        value * value * value
    } else {
        3.0 * DELTA * DELTA * (value - 4.0 / 29.0)
    }
}

pub(super) fn xyz_to_lab(xyz: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    let x = lab_f(xyz[0] / white[0]);
    let y = lab_f(xyz[1] / white[1]);
    let z = lab_f(xyz[2] / white[2]);
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

pub(super) fn lab_to_xyz(lab: [f64; 3], white: [f64; 3]) -> [f64; 3] {
    let y = (lab[0] + 16.0) / 116.0;
    let x = y + lab[1] / 500.0;
    let z = y - lab[2] / 200.0;
    [
        white[0] * lab_f_inverse(x),
        white[1] * lab_f_inverse(y),
        white[2] * lab_f_inverse(z),
    ]
}

pub(super) fn xyz_d50_to_lab_d50(xyz: &[f64]) -> Vec<f64> {
    xyz_to_lab([xyz[0], xyz[1], xyz[2]], ICC_D50).to_vec()
}

pub(super) fn lab_d50_to_xyz_d50(lab: &[f64]) -> Vec<f64> {
    lab_to_xyz([lab[0], lab[1], lab[2]], ICC_D50).to_vec()
}

pub(super) fn xyz_d50_to_lab_d50_wgsl(name: &str) -> String {
    format!(
        "fn {name}_f(value: f32) -> f32 {{\n\
             if value > 0.00885645168 {{ return pow(value, 1.0 / 3.0); }}\n\
             return value / 0.12841854935 + 0.13793103448;\n\
         }}\n\
         fn {name}(value: array<f32, 3>) -> array<f32, 3> {{\n\
             let x = {name}_f(value[0] / 0.9642);\n\
             let y = {name}_f(value[1]);\n\
             let z = {name}_f(value[2] / 0.8249);\n\
             return array<f32, 3>(116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z));\n\
         }}\n"
    )
}

pub(super) fn lab_d50_to_xyz_d50_wgsl(name: &str) -> String {
    format!(
        "fn {name}_inverse(value: f32) -> f32 {{\n\
             if value > 0.20689655172 {{ return value * value * value; }}\n\
             return 0.12841854935 * (value - 0.13793103448);\n\
         }}\n\
         fn {name}(value: array<f32, 3>) -> array<f32, 3> {{\n\
             let y = (value[0] + 16.0) / 116.0;\n\
             let x = y + value[1] / 500.0;\n\
             let z = y - value[2] / 200.0;\n\
             return array<f32, 3>(0.9642 * {name}_inverse(x), {name}_inverse(y), 0.8249 * {name}_inverse(z));\n\
         }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Transfer, TransferStage};

    #[test]
    fn d50_pcs_white_black_and_negative_roundtrip() {
        let white = xyz_d50_to_lab_d50(&ICC_D50);
        assert!((white[0] - 100.0).abs() < 1e-12);
        assert!(white[1].abs() < 1e-12);
        assert!(white[2].abs() < 1e-12);
        assert!(
            xyz_d50_to_lab_d50(&[0.0; 3])
                .iter()
                .all(|value| value.abs() < 1e-12)
        );
        for xyz in [[0.1, 0.2, 0.4], [-0.01, 0.02, 0.03], ICC_D50] {
            let restored = lab_d50_to_xyz_d50(&xyz_d50_to_lab_d50(&xyz));
            for (actual, expected) in restored.into_iter().zip(xyz) {
                assert!((actual - expected).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn pcs_shader_stages_parse_and_validate() {
        let transfer = Transfer::new(
            3,
            [TransferStage::XyzD50ToLabD50, TransferStage::LabD50ToXyzD50],
        )
        .unwrap();
        let source = transfer.wgsl(51).unwrap();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&source)));
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
        assert!(source.contains("theoria_transfer_51_pcs_0"));
        assert!(source.contains("theoria_transfer_51_pcs_1"));
    }
}
