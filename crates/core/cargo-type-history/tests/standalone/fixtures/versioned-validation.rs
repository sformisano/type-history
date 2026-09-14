mod validation {
    use history_api::Schema;
    use serde::{Deserialize, Deserializer, Serialize};
    use std::cell::Cell;

    thread_local! { static READS: Cell<u32> = const { Cell::new(0) }; }

    #[derive(Clone, Debug, PartialEq, Serialize, Schema)]
    #[serde(deny_unknown_fields)]
    pub struct Observed {
        pub value: u32,
    }

    impl<'de> Deserialize<'de> for Observed {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            READS.with(|reads| reads.set(reads.get() + 1));
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                value: u32,
            }
            let Fields { value } = Fields::deserialize(deserializer)?;
            Ok(Self { value })
        }
    }

    #[history_api::versioned(stable_name = "example.observed.record")]
    pub struct Checked {
        pub detail: Observed,
    }

    #[cfg(test)]
    mod tests {
        use super::{Checked, READS};
        use history_api::Versioned;

        #[test]
        fn invalid_metadata_never_runs_the_historical_field_deserializer() {
            let payload = r#"{"detail":{"value":42}}"#;
            for (case, tail) in [
                (
                    "wrong name",
                    r#", "version":1,"stable_name":"other.record""#,
                ),
                (
                    "future version",
                    r#", "version":2,"stable_name":"example.observed.record""#,
                ),
                ("missing name", r#", "version":1"#),
                (
                    "missing version",
                    r#", "stable_name":"example.observed.record""#,
                ),
                (
                    "zero version",
                    r#", "version":0,"stable_name":"example.observed.record""#,
                ),
                (
                    "duplicate version",
                    r#", "version":1,"stable_name":"example.observed.record","version":1"#,
                ),
                (
                    "duplicate name",
                    r#", "version":1,"stable_name":"example.observed.record","stable_name":"example.observed.record""#,
                ),
                (
                    "duplicate payload",
                    r#", "version":1,"stable_name":"example.observed.record","payload":{"detail":{"value":42}}"#,
                ),
                (
                    "unknown field",
                    r#", "version":1,"stable_name":"example.observed.record","extra":true"#,
                ),
            ] {
                READS.with(|reads| reads.set(0));
                let wire = format!(r#"{{"payload":{payload}{tail}}}"#);
                assert!(
                    serde_json::from_str::<Versioned<Checked>>(&wire).is_err(),
                    "accepted {case}"
                );
                READS.with(|reads| assert_eq!(reads.get(), 0, "deserialized {case}"));
            }
        }

        #[test]
        fn valid_metadata_runs_the_field_deserializer_once_in_every_order() {
            let fields = [
                r#""stable_name":"example.observed.record""#,
                r#""version":1"#,
                r#""payload":{"detail":{"value":42}}"#,
            ];
            for order in [
                [0, 1, 2],
                [0, 2, 1],
                [1, 0, 2],
                [1, 2, 0],
                [2, 0, 1],
                [2, 1, 0],
            ] {
                READS.with(|reads| reads.set(0));
                let wire = format!("{{{}}}", order.map(|index| fields[index]).join(","));
                let stored: Versioned<Checked> = serde_json::from_str(&wire).unwrap();
                READS.with(|reads| assert_eq!(reads.get(), 1));
                assert_eq!(Checked::from_versioned(stored).unwrap().detail.value, 42);
                READS.with(|reads| assert_eq!(reads.get(), 1));
            }
        }
    }
}
