//! Fuzz oracles for public codecs and the reference application's conversions.
#![forbid(unsafe_code)]

use invoice_history::Invoice;
use serde::Serialize;
use type_history::Versioned;
use type_history_core::resolved::SchemaShape;

#[derive(Serialize)]
#[serde(untagged)]
enum InputPayload<'a> {
    Initial {
        legacy: &'a str,
        count: u32,
    },
    Second {
        label: &'a str,
        count: u64,
        revision: u32,
    },
    Current {
        label: &'a str,
        count: &'a str,
        revision: u32,
    },
}

#[derive(Serialize)]
struct InputEnvelope<'a> {
    // Payload first also exercises buffering before the version is known.
    payload: InputPayload<'a>,
    version: u32,
    stable_name: &'static str,
}

/// Exercise arbitrary JSON without treating ordinary decoding errors as bugs.
pub fn invoice_json(bytes: &[u8]) {
    if let Ok(stored) = serde_json::from_slice::<Versioned<Invoice>>(bytes) {
        check_invoice(stored);
    }
}

/// Exercise arbitrary MessagePack, including its binary lengths and nesting.
pub fn invoice_msgpack(bytes: &[u8]) {
    if let Ok(stored) = rmp_serde::from_slice::<Versioned<Invoice>>(bytes) {
        check_invoice(stored);
    }
}

/// Generate valid values for every retained version and verify their meaning.
pub fn invoice_values(count: u32, revision: u32, label: &str) {
    let current_count = count.to_string();
    for (version, payload) in [
        (
            1,
            InputPayload::Initial {
                legacy: label,
                count,
            },
        ),
        (
            2,
            InputPayload::Second {
                label,
                count: u64::from(count),
                revision,
            },
        ),
        (
            3,
            InputPayload::Current {
                label,
                count: &current_count,
                revision,
            },
        ),
    ] {
        let mut envelope = InputEnvelope {
            stable_name: "billing.invoice.issued",
            version,
            payload,
        };
        let expected = serde_json::to_value(&envelope).unwrap();
        let json = serde_json::to_vec(&envelope).unwrap();
        let stored: Versioned<Invoice> = serde_json::from_slice(&json).unwrap();
        assert_eq!(serde_json::to_value(&stored).unwrap(), expected);
        check_invoice(stored);

        let binary = rmp_serde::to_vec_named(&envelope).unwrap();
        let stored: Versioned<Invoice> = rmp_serde::from_slice(&binary).unwrap();
        assert_eq!(serde_json::to_value(&stored).unwrap(), expected);
        check_invoice(stored);

        envelope.version = 4;
        assert!(serde_json::from_slice::<Versioned<Invoice>>(
            &serde_json::to_vec(&envelope).unwrap()
        )
        .is_err());
        assert!(rmp_serde::from_slice::<Versioned<Invoice>>(
            &rmp_serde::to_vec_named(&envelope).unwrap()
        )
        .is_err());
    }
}

fn check_invoice(stored: Versioned<Invoice>) {
    let original = serde_json::to_value(&stored).unwrap();
    let version = stored.source_version().get();
    let json = serde_json::to_vec(&stored).unwrap();
    let reread: Versioned<Invoice> = serde_json::from_slice(&json).unwrap();
    assert_eq!(serde_json::to_value(&reread).unwrap(), original);
    assert_eq!(reread.source_version().get(), version);

    let binary = rmp_serde::to_vec_named(&stored).unwrap();
    let reread: Versioned<Invoice> = rmp_serde::from_slice(&binary).unwrap();
    assert_eq!(serde_json::to_value(&reread).unwrap(), original);
    assert_eq!(reread.source_version().get(), version);

    // Compute the expected business result separately from generated callbacks.
    let payload = &original["payload"];
    let upgraded = Invoice::from_versioned(stored);
    if version < 3 && payload["count"].as_u64() == Some(13) {
        let error = upgraded.unwrap_err();
        assert_eq!(error.source_version().get(), version);
        assert_eq!(error.adjacent_from().unwrap().get(), 2);
        assert_eq!(error.adjacent_to().unwrap().get(), 3);
        return;
    }
    let expected = Invoice {
        count: if version < 3 {
            payload["count"].as_u64().unwrap().to_string()
        } else {
            payload["count"].as_str().unwrap().to_owned()
        },
        label: payload[if version == 1 { "legacy" } else { "label" }]
            .as_str()
            .unwrap()
            .to_owned(),
        revision: if version == 1 {
            7
        } else {
            u32::try_from(payload["revision"].as_u64().unwrap()).unwrap()
        },
    };
    assert_eq!(upgraded.unwrap(), expected);
}

/// Successful schema decoding must survive serialization and normalization.
pub fn schema_json(bytes: &[u8]) {
    if let Ok(shape) = serde_json::from_slice::<SchemaShape>(bytes) {
        let bytes = serde_json::to_vec(&shape).unwrap();
        assert_eq!(
            serde_json::from_slice::<SchemaShape>(&bytes).unwrap(),
            shape
        );
        let normalized = shape.normalized();
        assert_eq!(normalized.clone().normalized(), normalized);
    }
}

#[cfg(test)]
mod tests {
    use super::{invoice_json, invoice_msgpack, invoice_values, schema_json};
    use invoice_history::Invoice;
    use type_history::Versioned;
    use type_history_core::resolved::SchemaShape;

    #[test]
    fn generated_values_cover_versions_failures_and_string_boundaries() {
        for count in [0, 12, 13, 14, u32::MAX] {
            for label in ["", "invoice", "\"\\\n\0", "€🦀", &"x".repeat(4096)] {
                invoice_values(count, u32::MAX, label);
            }
        }
    }

    #[test]
    fn malformed_and_valid_seeds_exercise_public_oracles() {
        let json = include_bytes!("../seeds/invoice_json/v1.json");
        assert!(serde_json::from_slice::<Versioned<Invoice>>(json).is_ok());
        invoice_json(json);
        let binary = include_bytes!("../seeds/invoice_msgpack/v1.msgpack");
        assert!(rmp_serde::from_slice::<Versioned<Invoice>>(binary).is_ok());
        invoice_msgpack(binary);
        let schema = include_bytes!("../seeds/schema_json/variants.json");
        let shape = serde_json::from_slice::<SchemaShape>(schema).unwrap();
        assert!(matches!(shape, SchemaShape::Enum { variants } if variants.len() == 4));
        schema_json(schema);

        let fields = include_bytes!("../seeds/schema_json/field-contracts.json");
        let shape = serde_json::from_slice::<SchemaShape>(fields).unwrap();
        assert!(matches!(shape, SchemaShape::Record { fields } if fields.len() == 3));
        schema_json(fields);

        let duplicate = include_bytes!("../seeds/invoice_json/duplicate.json");
        assert!(serde_json::from_slice::<Versioned<Invoice>>(duplicate).is_err());
        assert!(rmp_serde::from_slice::<Versioned<Invoice>>(&[0xc1]).is_err());
        assert!(serde_json::from_slice::<SchemaShape>(b"{}").is_err());
    }
}
