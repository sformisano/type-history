use super::{Admission, Declaration, Invocation};
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
        declarations: vec![Declaration {
            invocation: invocation.clone(),
            module_path: vec!["consumer".into(), "records".into()],
            rust_name: "Record".into(),
        }],
    };
    fs::write(&path, serde_json::to_vec(&admission).unwrap()).unwrap();
    let (strict, declaration) = Admission::read(&path, &invocation).unwrap();
    assert!(strict);
    assert_eq!(declaration.module_path, ["consumer", "records"]);
    assert_eq!(declaration.rust_name, "Record");
    let hidden = Invocation {
        line: 2,
        ..invocation.clone()
    };
    assert!(Admission::read(&path, &hidden).is_err());
}
