use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub struct Charge {
    pub cents: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub enum State {
    Held { days: u32 },
    Paid,
}

#[versioned(stable_name = "billing.settlement")]
pub struct Settlement {
    pub charges: BTreeMap<String, Vec<Charge>>,
    pub state: State,
}

#[test]
fn checked_decoders_reject_unknown_nested_fields() {
    use history_api::{HasHistory, PayloadVersion, Versioned};
    #[derive(Serialize)]
    struct Payload<'a> {
        charges: BTreeMap<&'a str, Vec<BTreeMap<&'a str, u32>>>,
        state: BTreeMap<&'a str, BTreeMap<&'a str, u32>>,
    }
    #[derive(Serialize)]
    struct Envelope<'a> {
        stable_name: &'a str,
        version: u32,
        payload: Payload<'a>,
    }

    for extra in [None, Some("fee_cents"), Some("fee_days")] {
        let mut charge = BTreeMap::from([("cents", 1200)]);
        let mut held = BTreeMap::from([("days", 2)]);
        match extra {
            Some("fee_cents") => {
                charge.insert("fee_cents", 30);
            }
            Some("fee_days") => {
                held.insert("fee_days", 30);
            }
            _ => {}
        }
        let envelope = Envelope {
            stable_name: "billing.settlement",
            version: 1,
            payload: Payload {
                charges: BTreeMap::from([("arbitrary-key", vec![charge])]),
                state: BTreeMap::from([("Held", held)]),
            },
        };
        let bytes = serde_json::to_vec(&envelope.payload).unwrap();
        let raw = Settlement::history().decode(PayloadVersion::INITIAL, &bytes);
        let json = serde_json::from_slice::<Versioned<Settlement>>(
            &serde_json::to_vec(&envelope).unwrap(),
        );
        let binary = rmp_serde::from_slice::<Versioned<Settlement>>(
            &rmp_serde::to_vec_named(&envelope).unwrap(),
        );
        if let Some(extra) = extra {
            assert!(raw.is_err());
            assert!(json.err().unwrap().to_string().contains(extra));
            assert!(binary.err().unwrap().to_string().contains(extra));
        } else {
            let expected = raw.unwrap();
            assert_eq!(expected.charges["arbitrary-key"][0].cents, 1200);
            for decoded in [json.unwrap(), binary.unwrap()] {
                assert_eq!(Settlement::from_versioned(decoded).unwrap(), expected);
            }
        }
    }
}
