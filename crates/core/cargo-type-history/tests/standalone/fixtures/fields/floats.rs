use history_api::versioned;
use float_native::NonNaNFinite;
use std::collections::BTreeMap;

pub type Finite32 = NonNaNFinite<f32>;
pub type Finite64 = NonNaNFinite<f64>;

#[versioned(stable_name = "fields.floats")]
pub struct Floats {
    pub single: Finite32,
    pub double: Finite64,
    pub nested: BTreeMap<String, (Option<Finite32>, Finite64)>,
}

#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::common::{MetadataFirst, PayloadFirst};
    use super::{Finite32, Finite64, Floats};
    use history_api::Versioned;
    use serde::Serialize;
    use std::collections::BTreeMap;

    fn same(actual: Floats, f: f32, d: f64) {
        assert_eq!(actual.single.get().to_bits(), f.to_bits());
        assert_eq!(actual.double.get().to_bits(), d.to_bits());
        let (single, double) = actual.nested["value"];
        assert_eq!(single.unwrap().get().to_bits(), f.to_bits());
        assert_eq!(double.get().to_bits(), d.to_bits());
    }

    fn envelope<S: Serialize>(value: &S, f: f32, d: f64) {
        let json: Versioned<Floats> = serde_json::from_slice(&serde_json::to_vec(value).unwrap()).unwrap();
        let binary: Versioned<Floats> = rmp_serde::from_slice(&rmp_serde::to_vec_named(value).unwrap()).unwrap();
        for stored in [json, binary] { same(Floats::from_versioned(stored).unwrap(), f, d); }
    }

    fn exercise(f: f32, d: f64) {
        let single = Finite32::try_from(f).unwrap();
        let double = Finite64::try_from(d).unwrap();
        let direct: Finite32 = serde_json::from_slice(&serde_json::to_vec(&single).unwrap()).unwrap();
        assert_eq!(direct.get().to_bits(), f.to_bits());
        let direct: Finite64 = serde_json::from_slice(&serde_json::to_vec(&double).unwrap()).unwrap();
        assert_eq!(direct.get().to_bits(), d.to_bits());
        let direct: Finite32 = rmp_serde::from_slice(&rmp_serde::to_vec(&single).unwrap()).unwrap();
        assert_eq!(direct.get().to_bits(), f.to_bits());
        let direct: Finite64 = rmp_serde::from_slice(&rmp_serde::to_vec(&double).unwrap()).unwrap();
        assert_eq!(direct.get().to_bits(), d.to_bits());
        let payload = Floats { single, double, nested: [("value".into(), (Some(single), double))].into() };
        let stable_name = "fields.floats";
        envelope(&payload.clone().into_versioned(), f, d);
        envelope(&PayloadFirst { payload: &payload, version: 1, stable_name }, f, d);
        envelope(&MetadataFirst { payload, version: 1, stable_name }, f, d);
    }

    #[test]
    fn field_support_float_bits_survive_direct_and_buffered_codecs() {
        for (f, d) in [
            (0.0, 0.0), (-0.0, -0.0), (f32::from_bits(1), f64::from_bits(1)),
            (f32::from_bits(0x007f_ffff), f64::from_bits(0x000f_ffff_ffff_ffff)),
            (f32::MIN_POSITIVE, f64::MIN_POSITIVE), (f32::MAX, f64::MAX),
            (f32::MIN, f64::MIN), (0.1, 0.1),
            (f32::from_bits(0x0080_0001), f64::from_bits(0x0010_0000_0000_0001)),
        ] { exercise(f, d); }
        let mut state = 0x7b91_547e_ca33_506d_u64;
        let mut samples = 0;
        while samples < 512 {
            state ^= state << 13; state ^= state >> 7; state ^= state << 17;
            let d = f64::from_bits(state);
            let f = f32::from_bits((state ^ (state >> 32)) as u32);
            if f.is_finite() && d.is_finite() { exercise(f, d); samples += 1; }
        }
    }

    #[test]
    fn field_support_nonfinite_values_are_errors() {
        for raw in ["1e9999", "-1e9999", "NaN", "Infinity"] {
            let input = format!(r#"{{"payload":{{"single":{raw},"double":0,"nested":{{}}}},"version":1,"stable_name":"fields.floats"}}"#);
            assert!(serde_json::from_str::<Versioned<Floats>>(&input).is_err());
            assert!(serde_json::from_str::<Finite32>(raw).is_err());
            assert!(serde_json::from_str::<Finite64>(raw).is_err());
        }
        #[derive(Serialize)]
        struct Raw { single: f32, double: f64, nested: BTreeMap<String, ()> }
        for (single, double) in [(f32::NAN, f64::NAN), (f32::INFINITY, f64::INFINITY), (f32::NEG_INFINITY, f64::NEG_INFINITY)] {
            let raw = Raw { single, double, nested: Default::default() };
            let bytes = rmp_serde::to_vec_named(&PayloadFirst { payload: raw, version: 1, stable_name: "fields.floats" }).unwrap();
            assert!(rmp_serde::from_slice::<Versioned<Floats>>(&bytes).is_err());
            assert!(rmp_serde::from_slice::<Finite32>(&rmp_serde::to_vec(&single).unwrap()).is_err());
            assert!(rmp_serde::from_slice::<Finite64>(&rmp_serde::to_vec(&double).unwrap()).is_err());
        }
    }
}
