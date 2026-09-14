use super::{Admission, Invocation};
use std::fs;

#[test]
fn missing_malformed_and_unmatched_declarations_reject() {
    let owner = tempfile::tempdir().unwrap();
    let path = owner.path().join("admission.json");
    let invocation = Invocation {
        path: owner.path().join("lib.rs"),
        line: 1,
        column: 0,
        stable_name: "checked.record".into(),
    };
    assert!(Admission::read(&path, &invocation).is_err());
    fs::write(&path, b"{").unwrap();
    assert!(Admission::read(&path, &invocation).is_err());
    let admission = Admission {
        strict: true,
        invocations: vec![invocation.clone()],
    };
    fs::write(&path, serde_json::to_vec(&admission).unwrap()).unwrap();
    assert!(Admission::read(&path, &invocation).unwrap().strict);
    let hidden = Invocation {
        line: 2,
        ..invocation.clone()
    };
    assert!(Admission::read(&path, &hidden).is_err());
}
