use crate::decode_json_payload;
use serde::Deserialize;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Pair {
    first: u32,
    second: u32,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Containers {
    record: Pair,
    list: Vec<Pair>,
    optional: Option<Pair>,
    fixed: [Pair; 1],
    numbers: [u32; 2],
}

#[test]
fn named_records_remain_maps_inside_each_supported_container() {
    let record = r#"{"first":1,"second":2}"#;
    let input = format!(
        r#"{{"record":{record},"list":[{record}],"optional":{record},"fixed":[{record}],"numbers":[1,2]}}"#
    );
    let actual: Containers = decode_json_payload(input.as_bytes()).unwrap();
    assert_eq!(
        actual,
        Containers {
            record: Pair {
                first: 1,
                second: 2
            },
            list: vec![Pair {
                first: 1,
                second: 2
            }],
            optional: Some(Pair {
                first: 1,
                second: 2
            }),
            fixed: [Pair {
                first: 1,
                second: 2
            }],
            numbers: [1, 2],
        }
    );
    for field in ["record", "list", "optional", "fixed"] {
        let (valid, invalid) = match field {
            "list" | "fixed" => (format!("[{record}]"), "[[1,2]]"),
            _ => (record.to_owned(), "[1,2]"),
        };
        let bad = input.replace(
            &format!("\"{field}\":{valid}"),
            &format!("\"{field}\":{invalid}"),
        );
        assert_ne!(bad, input);
        let error = decode_json_payload::<Containers>(bad.as_bytes()).unwrap_err();
        assert!(
            error.to_string().contains("named fields"),
            "{field}: {error}"
        );
    }
}

#[test]
fn raw_json_errors_keep_the_decoder_error_and_reject_trailing_input() {
    for invalid in [
        br#"{"first":1,"first":2,"second":3}"#.as_slice(),
        br#"{"first":1,"second":2} trailing"#,
        br#"{"first":1,"second":2,"extra":3}"#,
    ] {
        assert!(decode_json_payload::<Pair>(invalid).is_err());
    }
    let error = decode_json_payload::<Pair>(b"{broken").unwrap_err();
    assert!(error.is_syntax());
    assert_eq!(error.line(), 1);
    assert!(error.column() > 0);
}
