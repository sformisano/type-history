use super::support::{success, Fixture};
use serde_json::Value;

#[test]
fn complete_large_file_reaches_rustc() {
    let fixture = Fixture::empty();
    success(&fixture.cli(&["init", "--package", "standalone-history-consumer"]));
    let count = 400;
    let stable = |index| format!("large.{}.record{index}", "a".repeat(235));
    let mut source = (0..count)
        .map(|index| {
            format!(
                "#[history_api::versioned(stable_name={:?})] pub struct Record{index} {{}}\n",
                stable(index)
            )
        })
        .collect::<String>();
    source.push_str(&format!("#[test] fn last_metadata() {{ use history_api::HasHistory; assert_eq!(Record399::STABLE_NAME.as_str(), {:?}); assert_eq!(Record399::VERSION.get(), 1); assert!(env!(\"TYPE_HISTORY_ADMISSION\").len() < 4096); }}", stable(count - 1)));
    fixture.write("src/lib.rs", &source);
    fixture.write("build.rs", r#"fn main() {
        use std::path::PathBuf;
        history_build::compile();
        let path=PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("type-history/admission.json");
        let bytes=std::fs::read(&path).unwrap();
        assert!(bytes.len()>132000, "actual admission length: {}",bytes.len());
        std::fs::write("admission-observed.json", &bytes).unwrap();
        std::fs::write("admission-observed-path", path.to_str().unwrap()).unwrap();
    }"#);
    success(&fixture.cargo(&["test", "--locked", "--offline", "last_metadata"]));
    let bytes = fixture.read("admission-observed.json");
    let actual: Value = serde_json::from_str(&bytes).unwrap();
    let entries = actual["declarations"].as_array().unwrap();
    assert!(bytes.len() > 132000);
    assert_eq!(entries.len(), count);
    let path = fixture.read("admission-observed-path");
    assert!(path.len() < 4096);
    for index in [0, count / 2, count - 1] {
        assert!(entries
            .iter()
            .any(|entry| entry["invocation"]["stable_name"] == stable(index)));
    }
    println!(
        "large admission: bytes={}, identities={}, path bytes={}",
        bytes.len(),
        entries.len(),
        path.len()
    );
}
