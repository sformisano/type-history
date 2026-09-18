use super::{BufferedValue, Content};
use crate::decode_json_payload;
use serde::Deserialize;
use typed_floats::NonNaNFinite;

#[derive(Deserialize)]
struct Single32 {
    value: NonNaNFinite<f32>,
}
#[derive(Deserialize)]
struct Single64 {
    value: NonNaNFinite<f64>,
}

#[test]
fn requested_width_replay_avoids_f32_double_rounding() {
    // Just above an f32 midpoint, but rounds exactly to that midpoint in f64.
    let text = "1.0000000596046447753906250000000001";
    let expected = text.parse::<f32>().unwrap();
    assert_ne!(
        expected.to_bits(),
        (text.parse::<f64>().unwrap() as f32).to_bits()
    );
    let json = format!("{{\"value\":{text}}}");
    let decoded: Single32 = decode_json_payload(json.as_bytes()).unwrap();
    assert_eq!(decoded.value.get().to_bits(), expected.to_bits());
}

#[test]
fn finite_widths_replay_signed_zero_boundaries_and_generated_bits() {
    let mut bits32 = 0x8a2d_49cbu32;
    let samples32 = [0, 0x8000_0000, 1, 0x007f_ffff, 0x0080_0000, 0x7f7f_ffff];
    for bits in samples32.into_iter().chain((0..2048).map(|_| {
        bits32 = bits32.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        bits32
    })) {
        let value = f32::from_bits(bits);
        if !value.is_finite() {
            continue;
        }
        let literal = serde_json::to_string(&value).unwrap();
        let json = format!("{{\"value\":{literal}}}");
        let decoded: Single32 = decode_json_payload(json.as_bytes()).unwrap();
        assert_eq!(decoded.value.get().to_bits(), bits, "{literal}");
    }
    let mut bits64 = 0x51f3_e20a_aab7_0b41u64;
    let samples64 = [
        0,
        0x8000_0000_0000_0000,
        1,
        0x000f_ffff_ffff_ffff,
        0x0010_0000_0000_0000,
        0x7fef_ffff_ffff_ffff,
    ];
    for bits in samples64.into_iter().chain((0..2048).map(|_| {
        bits64 = bits64
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        bits64
    })) {
        let value = f64::from_bits(bits);
        if !value.is_finite() {
            continue;
        }
        let literal = serde_json::to_string(&value).unwrap();
        let json = format!("{{\"value\":{literal}}}");
        let decoded: Single64 = decode_json_payload(json.as_bytes()).unwrap();
        assert_eq!(decoded.value.get().to_bits(), bits, "{literal}");
    }
}

#[test]
fn finite_replay_rejects_overflow_and_nonfinite_binary_values() {
    assert!(decode_json_payload::<Single32>(br#"{"value":3.5e38}"#).is_err());
    assert!(decode_json_payload::<Single64>(br#"{"value":1e309}"#).is_err());
    for value in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
        let buffer = || BufferedValue {
            value: Content::Float(value),
            human_readable: false,
        };
        assert!(NonNaNFinite::<f32>::deserialize(buffer()).is_err());
        assert!(NonNaNFinite::<f64>::deserialize(buffer()).is_err());
    }
}
