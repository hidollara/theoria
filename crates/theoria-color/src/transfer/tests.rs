use super::evaluate::{lab_d50_to_xyz_d50, xyz_d50_to_lab_d50};
use super::*;

#[test]
fn d50_pcs_white_black_and_negative_roundtrip() {
    let white = xyz_d50_to_lab_d50(&ICC_PCS_D50_XYZ);
    assert!((white[0] - 100.0).abs() < 1e-12);
    assert!(white[1].abs() < 1e-12);
    assert!(white[2].abs() < 1e-12);
    assert!(
        xyz_d50_to_lab_d50(&[0.0; 3])
            .iter()
            .all(|value| value.abs() < 1e-12)
    );
    for xyz in [[0.1, 0.2, 0.4], [-0.01, 0.02, 0.03], ICC_PCS_D50_XYZ] {
        let restored = lab_d50_to_xyz_d50(&xyz_d50_to_lab_d50(&xyz));
        for (actual, expected) in restored.into_iter().zip(xyz) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }
}

#[test]
fn directed_stages_evaluate_in_order_and_change_channel_count() {
    let transfer = Transfer::new(
        3,
        [
            TransferStage::curves([
                TransferCurve::gamma(2.0).unwrap(),
                TransferCurve::IDENTITY,
                TransferCurve::IDENTITY,
            ])
            .unwrap(),
            TransferStage::Matrix(
                TransferMatrix::new(3, 2, [1.0, 0.5, 0.0, 0.0, 0.0, 1.0], [0.1, 0.0]).unwrap(),
            ),
        ],
    )
    .unwrap();
    assert_eq!(transfer.input_channels(), 3);
    assert_eq!(transfer.output_channels(), 2);
    assert_eq!(
        transfer.evaluate(&[0.5, 0.25, 0.75]).unwrap(),
        vec![0.475, 0.75]
    );
}

#[test]
fn sampled_curves_interpolate_and_clamp_the_icc_domain() {
    let curve = TransferCurve::sampled([0.0, 0.25, 1.0]).unwrap();
    assert_eq!(curve.evaluate(-1.0), 0.0);
    assert_eq!(curve.evaluate(0.25), 0.125);
    assert_eq!(curve.evaluate(0.75), 0.625);
    assert_eq!(curve.evaluate(2.0), 1.0);
}

#[test]
fn multidimensional_clut_uses_icc_channel_order_and_interpolates() {
    let clut = TransferClut::new([2, 2], 1, [0.0, 0.4, 0.6, 1.0]).unwrap();
    let transfer = Transfer::new(2, [TransferStage::Clut(clut)]).unwrap();
    assert_eq!(transfer.evaluate(&[0.0, 1.0]).unwrap(), vec![0.4]);
    assert_eq!(transfer.evaluate(&[1.0, 0.0]).unwrap(), vec![0.6]);
    assert!((transfer.evaluate(&[0.25, 0.75]).unwrap()[0] - 0.45).abs() < 1e-12);
    assert_eq!(transfer.evaluate(&[2.0, -1.0]).unwrap(), vec![0.6]);
}

#[test]
fn piecewise_curves_preserve_inclusive_and_exclusive_boundaries() {
    let curve = TransferCurve::piecewise([
        CurveSegment::exclusive(0.5, CurveSegmentKind::Constant(0.0)),
        CurveSegment::new(f64::INFINITY, CurveSegmentKind::Constant(1.0)),
    ])
    .unwrap();
    assert_eq!(curve.evaluate(0.499), 0.0);
    assert_eq!(curve.evaluate(0.5), 1.0);
    let curve = TransferCurve::clamped(curve, [0.0, 1.0], [0.0, 0.75]).unwrap();
    assert_eq!(curve.evaluate(2.0), 0.75);
    assert!(
        TransferCurve::piecewise([CurveSegment::new(0.5, CurveSegmentKind::Constant(0.0),)])
            .is_err()
    );
}

#[test]
fn piecewise_formulas_evaluate_without_icc_function_codes() {
    let logarithmic = TransferCurve::piecewise([CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::Logarithmic {
            exponent: 1.0,
            input_scale: 1.0,
            input_offset: 1.0,
            output_scale: 1.0,
            output_offset: 0.0,
        },
    )])
    .unwrap();
    let exponential = TransferCurve::piecewise([CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::Exponential {
            scale: 1.0,
            base: 2.0,
            exponent_scale: 1.0,
            exponent_offset: 0.0,
            output_offset: 0.0,
        },
    )])
    .unwrap();
    assert_eq!(logarithmic.evaluate(9.0), 1.0);
    assert_eq!(exponential.evaluate(3.0), 8.0);
}

#[test]
fn named_srgb_curves_use_the_same_piecewise_representation() {
    let decoded_half = ((0.5_f64 + 0.055) / 1.055).powf(2.4);
    let encoded_half = 1.055 * 0.5_f64.powf(1.0 / 2.4) - 0.055;
    assert!((TransferCurve::SRGB_DECODE.evaluate(0.5) - decoded_half).abs() < 1e-12);
    assert!((TransferCurve::SRGB_ENCODE.evaluate(0.5) - encoded_half).abs() < 1e-12);

    for (named, threshold, expected, is_decode) in [
        (TransferCurve::SRGB_DECODE, 0.04045, 1.0 / 12.92, true),
        (TransferCurve::SRGB_ENCODE, 0.003_130_8, 12.92, false),
    ] {
        let TransferCurveKind::Piecewise(segments) = named.kind() else {
            panic!("a named sRGB curve must have a piecewise definition");
        };
        assert!(!segments.is_empty());
        let generic = TransferCurve::piecewise(segments.to_vec()).unwrap();
        assert_eq!(named.is_srgb_decode(), is_decode);
        assert_eq!(named.is_srgb_encode(), !is_decode);
        assert_eq!(named.clone().is_srgb_decode(), is_decode);
        assert!(!generic.is_srgb_decode() && !generic.is_srgb_encode());
        for value in [-2.0, -1.0, -threshold, 0.0, threshold, 0.5, 1.0, 2.0] {
            assert!((named.evaluate(value) - generic.evaluate(value)).abs() < 1e-12);
            assert!((named.evaluate(-value) + named.evaluate(value)).abs() < 1e-12);
        }
        assert!((named.evaluate(1.0e-5) / 1.0e-5 - expected).abs() < 1e-12);
    }
}

#[test]
fn invalid_transfer_data_is_rejected_before_evaluation() {
    assert!(TransferCurve::gamma(0.0).is_err());
    assert!(TransferCurve::sampled([0.0]).is_err());
    assert!(TransferCurve::sampled([0.0, f64::NAN]).is_err());
    assert!(TransferMatrix::new(3, 2, [0.0; 3], [0.0; 2]).is_err());
    assert!(
        Transfer::new(
            2,
            [TransferStage::curves([TransferCurve::IDENTITY]).unwrap()]
        )
        .is_err()
    );
}

#[test]
fn transfer_composition_rejects_mismatched_channel_counts() {
    let first = Transfer::new(2, Vec::<TransferStage>::new()).unwrap();
    let second = Transfer::new(3, Vec::<TransferStage>::new()).unwrap();
    assert_eq!(
        first.then(&second),
        Err(TransferError::ChannelMismatch {
            expected: 2,
            actual: 3,
        })
    );
}
