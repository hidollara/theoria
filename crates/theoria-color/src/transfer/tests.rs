use super::*;
use crate::space::{ColorSpace, LabD50, XyzD50};

#[test]
fn calculator_changes_channels_and_evaluates_only_the_selected_branch() {
    use CalculatorOperation::*;
    let transfer = Transfer::calculator(
        2,
        1,
        [
            Input(0),
            Constant(0.0),
            Binary(CalculatorBinary::Greater),
            IfElse {
                when_true: vec![Input(1), Unary(CalculatorUnary::SquareRoot)].into(),
                when_false: vec![Constant(7.0)].into(),
            },
        ],
    )
    .unwrap();
    assert!(transfer.supports_channels(2, 1));
    assert_eq!(transfer.evaluate(&[1.0, 9.0]).unwrap(), [3.0]);
    assert_eq!(transfer.evaluate(&[-1.0, -9.0]).unwrap(), [7.0]);
    assert_eq!(
        transfer.evaluate(&[1.0, -9.0]),
        Err(TransferError::NonFiniteValue)
    );
    let matrix = Transfer::matrix([[2.0]], [1.0]).unwrap();
    assert_eq!(
        transfer
            .then(&matrix)
            .unwrap()
            .evaluate(&[1.0, 9.0])
            .unwrap(),
        [7.0]
    );
}

#[test]
fn calculator_rejects_invalid_programs_at_construction() {
    use CalculatorOperation::*;
    for operations in [
        vec![Input(1)],
        vec![Discard],
        vec![Duplicate(0)],
        vec![Unary(CalculatorUnary::Sin)],
        vec![Constant(1.0), Binary(CalculatorBinary::Add)],
        vec![
            Constant(1.0),
            IfElse {
                when_true: vec![Constant(1.0)].into(),
                when_false: vec![].into(),
            },
        ],
        vec![
            Constant(0.0),
            IfElse {
                when_true: vec![Input(1)].into(),
                when_false: vec![Constant(1.0)].into(),
            },
        ],
    ] {
        assert_eq!(
            Transfer::calculator(1, 1, operations),
            Err(TransferError::InvalidCalculator)
        );
    }
    assert!(matches!(
        Transfer::calculator(1, 2, [Input(0)]),
        Err(TransferError::ChannelMismatch { .. })
    ));
    assert_eq!(
        Transfer::calculator(1, 1, [Constant(f64::NAN)]),
        Err(TransferError::NonFiniteValue)
    );
    let mut nested = vec![Constant(1.0)];
    for _ in 0..66 {
        nested = vec![
            Constant(1.0),
            IfElse {
                when_true: nested.into(),
                when_false: vec![Constant(1.0)].into(),
            },
        ];
    }
    assert_eq!(
        Transfer::calculator(1, 1, nested),
        Err(TransferError::InvalidCalculator)
    );
}

#[test]
fn calculator_operand_order_stack_and_rounding_are_explicit() {
    use CalculatorOperation::*;
    let transfer = Transfer::calculator(
        2,
        4,
        [
            Input(0),
            Input(1),
            Binary(CalculatorBinary::Subtract),
            Duplicate(0),
            Constant(2.0),
            Binary(CalculatorBinary::Divide),
            Input(0),
            Input(1),
            Binary(CalculatorBinary::Atan2),
            Constant(-2.5),
            Unary(CalculatorUnary::Round),
            Constant(99.0),
            Discard,
        ],
    )
    .unwrap();
    assert_eq!(
        transfer.evaluate(&[3.0, 1.0]).unwrap(),
        [2.0, 1.0, 1.0_f64.atan2(3.0), -3.0]
    );
}

#[test]
fn d50_pcs_white_black_and_negative_roundtrip() {
    let xyz = ColorSpace::from(XyzD50);
    let lab = ColorSpace::from(LabD50);
    let to_lab = xyz.transfer_to(&lab).unwrap();
    let to_xyz = lab.transfer_to(&xyz).unwrap();
    assert_eq!(to_lab.stages().len(), 3);
    assert!(matches!(to_lab.stages()[0], TransferStage::Matrix(_)));
    assert!(matches!(to_lab.stages()[1], TransferStage::Curves(_)));
    assert!(matches!(to_lab.stages()[2], TransferStage::Matrix(_)));
    let white = to_lab.evaluate(&ICC_PCS_D50_XYZ).unwrap();
    assert!((white[0] - 100.0).abs() < 1e-12);
    assert!(white[1].abs() < 1e-12);
    assert!(white[2].abs() < 1e-12);
    assert!(
        to_lab
            .evaluate(&[0.0; 3])
            .unwrap()
            .iter()
            .all(|value| value.abs() < 1e-12)
    );
    for xyz in [[0.1, 0.2, 0.4], [-0.01, 0.02, 0.03], ICC_PCS_D50_XYZ] {
        let restored = to_xyz.evaluate(&to_lab.evaluate(&xyz).unwrap()).unwrap();
        for (actual, expected) in restored.into_iter().zip(xyz) {
            assert!((actual - expected).abs() < 1e-12);
        }
    }
}

#[test]
fn directed_stages_evaluate_in_order_and_change_channel_count() {
    let transfer = Transfer::new([
        Transfer::curves([
            TransferCurve::gamma(2.0).unwrap(),
            TransferCurve::IDENTITY,
            TransferCurve::IDENTITY,
        ])
        .unwrap(),
        Transfer::matrix([[1.0, 0.5, 0.0], [0.0, 0.0, 1.0]], [0.1, 0.0]).unwrap(),
    ])
    .unwrap();
    assert_eq!(transfer.input_channels(), Some(3));
    assert_eq!(transfer.output_channels(), Some(2));
    assert_eq!(transfer.clone().stages(), transfer.stages());
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
fn signed_power_curves_preserve_extended_negative_values() {
    let curve = TransferCurve::signed_power(2.4).unwrap();
    for input in [-1.5_f64, -0.5, 0.0, 0.5, 1.5] {
        let expected = input.signum() * input.abs().powf(2.4);
        assert!((curve.evaluate(input) - expected).abs() < 1e-12);
    }
}

#[test]
fn multidimensional_clut_uses_icc_channel_order_and_interpolates() {
    let transfer = Transfer::clut([2, 2], 1, [0.0, 0.4, 0.6, 1.0]).unwrap();
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
    assert!(
        TransferCurve::piecewise([CurveSegment::new(0.5, CurveSegmentKind::Constant(0.0),)])
            .is_err()
    );
}

#[test]
fn clamp_curves_can_limit_input_and_output_independently() {
    let curve = Transfer::curves([TransferCurve::piecewise([CurveSegment::new(
        f64::INFINITY,
        CurveSegmentKind::Linear {
            scale: 2.0,
            offset: 0.25,
        },
    )])
    .unwrap()])
    .unwrap();
    let input_clamp = Transfer::curves([TransferCurve::clamp(0.0, 1.0).unwrap()]).unwrap();
    let output_clamp = Transfer::curves([TransferCurve::clamp(0.0, 0.75).unwrap()]).unwrap();
    let input_only = input_clamp.then(&curve).unwrap();
    let output_only = curve.then(&output_clamp).unwrap();
    let both = input_only.then(&output_clamp).unwrap();
    assert_eq!(input_only.evaluate(&[-0.5]).unwrap(), vec![0.25]);
    assert_eq!(input_only.evaluate(&[2.0]).unwrap(), vec![2.25]);
    assert_eq!(output_only.evaluate(&[-0.5]).unwrap(), vec![0.0]);
    assert_eq!(output_only.evaluate(&[2.0]).unwrap(), vec![0.75]);
    assert_eq!(both.evaluate(&[-0.5]).unwrap(), vec![0.25]);
    assert_eq!(both.evaluate(&[2.0]).unwrap(), vec![0.75]);

    let per_channel = Transfer::curves([
        TransferCurve::clamp(-2.0, 2.0).unwrap(),
        TransferCurve::IDENTITY,
        TransferCurve::clamp(1.0, 1.0).unwrap(),
    ])
    .unwrap();
    assert_eq!(
        per_channel.evaluate(&[-3.0, 3.0, 0.0]).unwrap(),
        vec![-2.0, 3.0, 1.0]
    );

    let one_sided = Transfer::curves([
        TransferCurve::clamp(0.0, f64::INFINITY).unwrap(),
        TransferCurve::clamp(f64::NEG_INFINITY, 1.0).unwrap(),
        TransferCurve::clamp(f64::NEG_INFINITY, f64::INFINITY).unwrap(),
    ])
    .unwrap();
    assert_eq!(
        one_sided.evaluate(&[-2.0, 2.0, -3.0]).unwrap(),
        vec![0.0, 1.0, -3.0]
    );
    assert_eq!(
        one_sided.evaluate(&[2.0, -2.0, 3.0]).unwrap(),
        vec![2.0, -2.0, 3.0]
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
    assert!((TransferCurve::srgb_decode().evaluate(0.5) - decoded_half).abs() < 1e-12);
    assert!((TransferCurve::srgb_encode().evaluate(0.5) - encoded_half).abs() < 1e-12);

    for (named, threshold, expected, is_decode) in [
        (TransferCurve::srgb_decode(), 0.04045, 1.0 / 12.92, true),
        (TransferCurve::srgb_encode(), 0.003_130_8, 12.92, false),
    ] {
        let TransferCurveKind::Piecewise(segments) = named.kind() else {
            panic!("a named sRGB curve must have a piecewise definition");
        };
        assert!(!segments.is_empty());
        let generic = TransferCurve::piecewise(segments.to_vec()).unwrap();
        assert_eq!(named.is_srgb_decode(), is_decode);
        assert_eq!(named.is_srgb_encode(), !is_decode);
        assert_eq!(named.clone().is_srgb_decode(), is_decode);
        assert_eq!(named, generic);
        assert_eq!(generic.is_srgb_decode(), is_decode);
        assert_eq!(generic.is_srgb_encode(), !is_decode);
        let mut changed = segments.to_vec();
        changed[0].kind = CurveSegmentKind::Constant(0.0);
        let changed = TransferCurve::piecewise(changed).unwrap();
        assert!(!changed.is_srgb_decode() && !changed.is_srgb_encode());
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
    assert!(TransferCurve::clamp(1.0, 0.0).is_err());
    assert!(TransferCurve::clamp(f64::NAN, 1.0).is_err());
    assert!(TransferCurve::clamp(0.0, f64::NAN).is_err());
    assert!(Transfer::matrix_from_flat(3, 2, [0.0; 3], [0.0; 2]).is_err());
    assert_eq!(
        Transfer::matrix::<0, 1>([[]], [0.0]),
        Err(TransferError::EmptyChannels)
    );
    assert_eq!(
        Transfer::matrix([[f64::NAN]], [0.0]),
        Err(TransferError::NonFiniteValue)
    );
    assert_eq!(
        Transfer::matrix([[1.0]], [f64::INFINITY]),
        Err(TransferError::NonFiniteValue)
    );
    assert_eq!(
        Transfer::clut([2, 2], 1, [0.0; 3]),
        Err(TransferError::InvalidClutShape)
    );
    assert_eq!(Transfer::curves([]), Err(TransferError::EmptyChannels));
    assert!(
        Transfer::new([
            Transfer::curves([TransferCurve::IDENTITY]).unwrap(),
            Transfer::curves(vec![TransferCurve::clamp(0.0, 1.0).unwrap(); 2]).unwrap(),
        ])
        .is_err()
    );
}

#[test]
fn static_and_dynamic_matrix_constructors_agree() {
    let rows = [[1.0, 0.5, 0.0], [0.0, 0.0, 1.0]];
    let offsets = [0.1, 0.0];
    assert_eq!(
        Transfer::matrix(rows, offsets),
        Transfer::matrix_from_flat(
            3,
            2,
            rows.into_iter().flatten().collect::<Vec<_>>(),
            offsets
        )
    );
}

#[test]
fn transfer_composition_rejects_mismatched_channel_counts() {
    let first = Transfer::curves(vec![TransferCurve::clamp(0.0, 1.0).unwrap(); 2]).unwrap();
    let second = Transfer::curves(vec![TransferCurve::clamp(0.0, 1.0).unwrap(); 3]).unwrap();
    assert_eq!(
        first.then(&second),
        Err(TransferError::ChannelMismatch {
            expected: 2,
            actual: 3,
        })
    );
    assert_eq!(
        Transfer::new([first, Transfer::IDENTITY, second]),
        Err(TransferError::ChannelMismatch {
            expected: 2,
            actual: 3,
        })
    );
}

#[test]
fn empty_transfer_is_a_channel_polymorphic_identity() {
    let identity = Transfer::IDENTITY;
    assert_eq!(Transfer::new([]).unwrap(), identity);
    assert_eq!(identity.input_channels(), None);
    assert_eq!(identity.output_channels(), None);
    assert!(identity.supports_channels(2, 2));
    assert!(identity.supports_channels(3, 3));
    assert!(!identity.supports_channels(2, 3));
    assert_eq!(
        identity.evaluate(&[0.2, 0.5, 0.7]).unwrap(),
        vec![0.2, 0.5, 0.7]
    );

    for channels in [2, 3] {
        let transfer =
            Transfer::curves(vec![TransferCurve::clamp(0.0, 1.0).unwrap(); channels]).unwrap();
        assert_eq!(identity.then(&transfer).unwrap(), transfer);
        assert_eq!(transfer.then(&identity).unwrap(), transfer);
    }
}
