use history_api::{versioned, Schema, adapters::{DecimalText, UuidText}};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type Identifier = UuidText;
pub type Money = DecimalText;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub struct Wrapped(pub Option<Identifier>);
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum Choice { Empty, One(Money), Pair(Identifier, Money), Named { value: Wrapped } }

#[versioned(stable_name = "fields.logical")]
pub struct Logical {
    pub id: Identifier,
    pub amount: Money,
    pub nested: BTreeMap<String, (Option<Identifier>, Money)>,
    pub members: BTreeSet<Money>,
    pub choice: Choice,
}

// RUNTIME_TESTS_BEGIN
#[cfg(test)]
mod common;
#[cfg(test)]
mod tests {
    use super::common::PayloadFirst;
    use super::{common, Choice, Logical};
    use decimal_native::Decimal;
    use history_api::Versioned;
    use history_api::adapters::{DecimalText, UuidText};
    use serde_json::{json, Value};
    use std::collections::BTreeSet;
    use uuid_native::Uuid;

    fn expected(decimal: &str, uuid: &str) -> Logical {
        serde_json::from_value(json!({"id":uuid,"amount":decimal,"nested":{"value":[uuid,decimal]},"members":[decimal],"choice":{"Pair":[uuid,decimal]}})).unwrap()
    }
    fn same(value: Logical, decimal: &str, uuid: &str) {
        let native = Decimal::from_str_exact(decimal).unwrap();
        assert_eq!(value.amount.as_inner().mantissa(), native.mantissa());
        assert_eq!(value.amount.as_inner().scale(), native.scale());
        assert_eq!(serde_json::to_string(&value.amount).unwrap(), format!("\"{decimal}\""));
        assert_eq!(value.id.as_inner().as_u128(), Uuid::parse_str(uuid).unwrap().as_u128());
        assert_eq!(value.nested["value"].1.as_inner().scale(), native.scale());
        assert_eq!(value.members.first().unwrap().as_inner().scale(), native.scale());
        if let Choice::Pair(_, amount) = value.choice { assert_eq!(amount.as_inner().scale(), native.scale()); } else { panic!("variant changed"); }
    }

    #[test]
    fn field_support_logical_exact_values_through_both_codecs() {
        for uuid in [Uuid::nil(), Uuid::max(), Uuid::from_u128(0x123456789abcdef0123456789abcdef0)] {
            let text = uuid.hyphenated().to_string();
            let id = UuidText::try_from(uuid).unwrap();
            assert_eq!(id.clone().into_inner(), uuid);
            assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{text}\""));
            for decimal in ["0", "0.0000000000000000000000000000", "123.400", "-123.400", "79228162514264337593543950335", "-79228162514264337593543950335", "7.9228162514264337593543950335"] {
                let value = expected(decimal, &text);
                let native = Decimal::from_str_exact(decimal).unwrap();
                let amount = DecimalText::try_from(native).unwrap();
                assert_eq!(amount.clone().into_inner().scale(), native.scale());
                let versioned = value.clone().into_versioned();
                let json: Versioned<Logical> = serde_json::from_slice(&serde_json::to_vec(&versioned).unwrap()).unwrap();
                let binary: Versioned<Logical> = rmp_serde::from_slice(&rmp_serde::to_vec_named(&versioned).unwrap()).unwrap();
                for stored in [json, binary] { same(Logical::from_versioned(stored).unwrap(), decimal, &text); }
                let first = PayloadFirst { payload: value, version: 1, stable_name: "fields.logical" };
                let stored: Versioned<Logical> = serde_json::from_slice(&serde_json::to_vec(&first).unwrap()).unwrap();
                same(Logical::from_versioned(stored).unwrap(), decimal, &text);
            }
        }
        // Membership is numeric while stored scale remains exact.
        let a: DecimalText = serde_json::from_str("\"1.0\"").unwrap();
        let b: DecimalText = serde_json::from_str("\"1.00\"").unwrap();
        assert_eq!(a, b);
        assert_ne!(a.as_inner().scale(), b.as_inner().scale());
        assert_eq!(BTreeSet::from([a, b]).len(), 1);
    }

    #[test]
    fn field_support_logical_invalid_grammar_is_rejected() {
        let uuid = "00000000-0000-0000-0000-000000000000";
        for (field, invalid) in [
            ("id", "FFFFFFFF-FFFF-FFFF-FFFF-FFFFFFFFFFFF"), ("id", "00000000000000000000000000000000"),
            ("id", "g0000000-0000-0000-0000-000000000000"), ("id", "{00000000-0000-0000-0000-000000000000}"),
            ("amount", "1e2"), ("amount", "1_000"), ("amount", "+1"), ("amount", " 1"), ("amount", "1 "),
            ("amount", "1."), ("amount", ".1"), ("amount", "0.00000000000000000000000000000"),
            ("amount", "79228162514264337593543950336"), ("amount", "-79228162514264337593543950336"),
        ] {
            let mut payload = serde_json::to_value(expected("1.00", uuid)).unwrap();
            payload[field] = json!(invalid);
            let value = json!({"stable_name":"fields.logical","version":1,"payload":payload});
            assert!(serde_json::from_value::<Versioned<Logical>>(value.clone()).is_err(), "{value}");
            assert!(rmp_serde::from_slice::<Versioned<Logical>>(&common::messagepack(&value)).is_err(), "{value}");
        }
        for invalid in [Value::Null, json!(1), json!([]), json!({})] {
            assert!(serde_json::from_value::<UuidText>(invalid.clone()).is_err());
            assert!(serde_json::from_value::<DecimalText>(invalid).is_err());
        }
    }
}
// RUNTIME_TESTS_END
