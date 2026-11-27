use float_native::NonNaNFinite;
use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub enum State {
    Ready,
    Note(Option<String>),
}

#[versioned(stable_name = "fields.json")]
pub struct Record {
    pub labels: BTreeMap<String, String>,
    pub count: u128,
    pub delta: i128,
    pub rates: (NonNaNFinite<f32>, NonNaNFinite<f64>),
    pub state: State,
}

#[cfg(test)]
mod common;

#[cfg(test)]
mod tests {
    use super::{Record, State};
    use history_api::{decode, HasHistory, PayloadVersion, Versioned};

    const LABELS: &str = r#"{"$serde_json::private::Number":"42"}"#;
    const MAX: &str = "340282366920938463463374607431768211455";
    const MIN: &str = "-170141183460469231731687303715884105728";

    fn payload(labels: &str, count: &str, state: &str) -> String {
        format!(
            r#"{{"labels":{labels},"count":{count},"delta":{MIN},"state":{state},"rates":[-0,-0]}}"#
        )
    }

    fn envelope(payload: &str) -> String {
        format!(r#"{{"payload":{payload},"version":1,"stable_name":"fields.json"}}"#)
    }

    fn check(value: Record) {
        assert_eq!(value.labels.len(), 1);
        assert_eq!(value.labels["$serde_json::private::Number"], "42");
        assert_eq!(value.count, u128::MAX);
        assert_eq!(value.delta, i128::MIN);
        assert_eq!(value.rates.0.get().to_bits(), (-0.0_f32).to_bits());
        assert_eq!(value.rates.1.get().to_bits(), (-0.0_f64).to_bits());
        assert_eq!(value.state, State::Ready);
    }

    #[test]
    fn genuine_objects_and_exact_numbers_stay_distinct_in_both_read_apis() {
        for labels in [LABELS, r#"{"\u0024serde_json::private::Number":"42"}"#] {
            let body = payload(labels, MAX, r#""Ready""#);
            let raw = decode::<Record>(
                &Record::STABLE_NAME,
                PayloadVersion::INITIAL,
                body.as_bytes(),
            )
            .unwrap();
            check(raw.clone());
            let wrapped = envelope(&body);
            let bytes = wrapped.as_bytes();
            let slice = serde_json::from_slice(bytes).unwrap();
            let reader = serde_json::from_reader(bytes).unwrap();
            #[derive(serde::Deserialize)]
            struct Parent {
                record: Versioned<Record>,
            }
            let nested = format!(r#"{{"record":{wrapped}}}"#);
            let parent: Parent = serde_json::from_reader(nested.as_bytes()).unwrap();
            let stored = raw.into_versioned();
            let tree = serde_json::to_value(&stored).unwrap();
            assert!(tree["payload"]["labels"].is_object());
            let from_tree = serde_json::from_value(tree).unwrap();
            let packed = rmp_serde::to_vec_named(&stored).unwrap();
            let from_msgpack = rmp_serde::from_slice(&packed).unwrap();
            for result in [slice, reader, parent.record, from_tree, from_msgpack] {
                check(Record::from_versioned(result).unwrap());
            }
        }
    }

    fn reject(body: &str) {
        assert!(
            decode::<Record>(
                &Record::STABLE_NAME,
                PayloadVersion::INITIAL,
                body.as_bytes()
            )
            .is_err(),
            "{body}"
        );
        let wrapped = envelope(body);
        assert!(
            serde_json::from_slice::<Versioned<Record>>(wrapped.as_bytes()).is_err(),
            "{wrapped}"
        );
        assert!(
            serde_json::from_reader::<_, Versioned<Record>>(wrapped.as_bytes()).is_err(),
            "{wrapped}"
        );
    }

    #[test]
    fn wrong_wire_kinds_fail_for_raw_and_payload_first_reads() {
        for number in ["12.34", MAX, "1e9999"] {
            reject(&payload(number, MAX, r#""Ready""#));
            reject(number);
            let mut tree = serde_json::from_str::<serde_json::Value>(&envelope(&payload(
                "{}",
                MAX,
                r#""Ready""#,
            )))
            .unwrap();
            tree["payload"]["labels"] = serde_json::from_str(number).unwrap();
            assert!(serde_json::from_value::<Versioned<Record>>(tree).is_err());
        }
        reject(&payload(LABELS, LABELS, r#""Ready""#));
        reject(&payload(LABELS, MAX, r#"{"Ready":null}"#));
        // Null is still a valid payload for a newtype variant around Option.
        let valid = payload(LABELS, MAX, r#"{"Note":null}"#);
        let wrapped = serde_json::from_str(&envelope(&valid)).unwrap();
        assert_eq!(
            Record::from_versioned(wrapped).unwrap().state,
            State::Note(None)
        );
        let raw = decode::<Record>(
            &Record::STABLE_NAME,
            PayloadVersion::INITIAL,
            valid.as_bytes(),
        )
        .unwrap();
        assert_eq!(raw.state, State::Note(None));
    }

    #[test]
    fn named_messagepack_requires_unit_variants_to_be_strings() {
        let mut value = serde_json::json!({
            "labels": {}, "count": 1, "delta": -1, "rates": [0, 0], "state": "Ready"
        });
        let valid = super::common::payload_first_binary("fields.json", &value);
        let record = Record::from_versioned(rmp_serde::from_slice(&valid).unwrap()).unwrap();
        assert_eq!(record.state, State::Ready);
        value["state"] = serde_json::json!({"Ready": null});
        let invalid = super::common::payload_first_binary("fields.json", &value);
        assert!(rmp_serde::from_slice::<Versioned<Record>>(&invalid).is_err());
    }

    #[test]
    fn untagged_serde_buffers_reject_erased_numeric_tokens() {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Outer {
            Historical(Versioned<Record>),
        }

        let simple = |labels| {
            payload(labels, "1", r#""Ready""#)
                .replace(MIN, "-1")
                .replace("[-0,-0]", "[0,0]")
        };
        let valid = envelope(&simple(r#"{"ordinary":"kept"}"#));
        let Outer::Historical(value) = serde_json::from_str(&valid).unwrap();
        assert_eq!(
            Record::from_versioned(value).unwrap().labels["ordinary"],
            "kept"
        );
        let ambiguous = envelope(&simple("1.5"));
        assert!(serde_json::from_str::<Outer>(&ambiguous).is_err());
    }
}
