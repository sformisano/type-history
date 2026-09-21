//! Frozen reconstruction and dependency invalidation through public Cargo builds.

use super::super::support::{failure as assert_failure, success as assert_success, Fixture};
use super::{fixture, ledger, source};
use type_history_core::resolved::SchemaShape;

#[test]
fn history_frozen_reconstruction() {
    let payload = r#"
         label: Label,
         #[history(added_in = v2, backfill_value = 0)] count: u32,
    "#;
    let baseline = ledger(vec![
        vec![("label", SchemaShape::String)],
        vec![("label", SchemaShape::String), ("count", SchemaShape::U32)],
    ]);
    let original = source(payload, "type Label = String;", "");
    let mislabeled = payload.replace(
        "added_in = v2, backfill_value = 0",
        "updated_in = v2, previous_type = u16, backfill_fn = widen",
    );
    let missing = payload.replace(
        "label: Label",
        "#[history(added_in = v2, backfill_value = String::new())] label: Label",
    );
    let cases = [
        (
            "alias wire drift",
            original.replace("type Label = String;", "type Label = u64;"),
            "billing.invoice.issued V1 field `label` changed its frozen wire shape",
        ),
        (
            "mislabeled update",
            source(
                &mislabeled,
                "type Label = String; fn widen(previous_payload: &RecordV1)->Result<u32,Infallible>{ let value = previous_payload.count.clone();Ok(u32::from(value))}",
                "",
            ),
            "billing.invoice.issued V1 field `count` did not exist in frozen history",
        ),
        (
            "mislabeled addition",
            source(&missing, "type Label = String;", ""),
            "billing.invoice.issued V1 field `label` disappeared from frozen history",
        ),
    ];
    let reordered =
        r#" #[history(added_in = v2, backfill_value = 0)] count: u32,  label: OtherLabel,"#;
    let equivalent = source(reordered, "type OtherLabel = String;", "");
    for command in ["check", "build"] {
        // Each fresh package has a distinct source path and no prior compiler invocation.
        // Dependency artifacts are reused, as required by the workstation storage policy.
        for (name, changed, diagnostic) in &cases {
            let fixture = fixture(changed);
            fixture.set_ledger(&baseline);
            frozen_command(
                &fixture,
                command,
                Some(diagnostic),
                &format!("fresh {name}"),
                &[],
            );
        }
        {
            let fixture = fixture(&equivalent);
            fixture.set_ledger(&baseline);
            frozen_command(
                &fixture,
                command,
                None,
                "fresh equivalent alias/reorder",
                &[],
            );
        }
        let fixture = fixture(&original);
        fixture.set_ledger(&baseline);
        frozen_command(&fixture, command, None, "warm baseline", &[]);
        for (name, changed, diagnostic) in &cases {
            fixture.write("src/lib.rs", changed);
            frozen_command(
                &fixture,
                command,
                Some(diagnostic),
                &format!("warm {name}"),
                &[],
            );
            fixture.write("src/lib.rs", &equivalent);
            frozen_command(
                &fixture,
                command,
                None,
                "warm equivalent alias/reorder",
                &[],
            );
        }
    }
}

/// Run a public compiler operation and prove that all authored inputs stay unchanged.
fn frozen_command(
    fixture: &Fixture,
    command: &str,
    diagnostic: Option<&str>,
    case: &str,
    extra_paths: &[&str],
) {
    let mut paths = vec![
        "Cargo.toml",
        "Cargo.lock",
        "type-history/schemas.json",
        "build.rs",
        "src/lib.rs",
    ];
    paths.extend_from_slice(extra_paths);
    let before: Vec<_> = paths
        .iter()
        .map(|path| std::fs::read(fixture.root().join(path)).expect("authored input"))
        .collect();
    let output = fixture.cargo(&[command, "--locked", "--offline"]);
    if let Some(diagnostic) = diagnostic {
        assert_failure(&output, diagnostic);
        let text = super::super::support::text(&output);
        assert!(
            !text.contains("error[E0308]"),
            "callback/type mismatch masked the frozen assertion: {text}"
        );
        assert!(
            !text.contains("error[E0425]"),
            "missing callback masked the frozen assertion: {text}"
        );
    } else {
        assert_success(&output);
    }
    for (path, bytes) in paths.iter().zip(before) {
        assert_eq!(
            std::fs::read(fixture.root().join(path)).expect("preserved input"),
            bytes,
            "cargo {command} changed {path} during {case}"
        );
    }
    eprintln!(
        "frozen proof: cargo {command}; {case}; expected {} observed; authored inputs unchanged",
        if diagnostic.is_some() {
            "frozen rejection"
        } else {
            "success"
        }
    );
}

#[test]
fn frozen_shape_nested_dependency_fresh_and_warm_check_and_build() {
    let payload = r#"
         labels: Vec<Option<wire_types::Label>>,
         #[history(added_in = v2, backfill_fn = initialize_count)] count: u32,
    "#;
    let input = source(
        payload,
        "fn initialize_count(_: &RecordV1) -> Result<u32,Infallible> { Ok(7) }",
        "",
    );
    let nested = SchemaShape::Sequence {
        value: Box::new(SchemaShape::Option {
            value: Box::new(SchemaShape::String),
        }),
    };
    let baseline = ledger(vec![
        vec![("labels", nested.clone())],
        vec![("labels", nested), ("count", SchemaShape::U32)],
    ]);
    let diagnostic = "billing.invoice.issued V1 field `labels` changed its frozen wire shape";
    let dependency_paths = [
        "wire-types/Cargo.toml",
        "wire-types/src/lib.rs",
        "wire-leaf/Cargo.toml",
        "wire-leaf/src/lib.rs",
    ];
    for command in ["check", "build"] {
        {
            let fixture = nested_dependency_fixture(&input, "pub type Label = u32;");
            fixture.set_ledger(&baseline);
            frozen_command(
                &fixture,
                command,
                Some(diagnostic),
                "fresh nested dependency alias drift",
                &dependency_paths,
            );
        }
        {
            let fixture = nested_dependency_fixture(
                &input,
                "type Renamed = String; pub type Label = Renamed;",
            );
            fixture.set_ledger(&baseline);
            frozen_command(
                &fixture,
                command,
                None,
                "fresh wire-equivalent dependency alias",
                &dependency_paths,
            );
        }
        let fixture = nested_dependency_fixture(&input, "pub type Label = String;");
        fixture.set_ledger(&baseline);
        frozen_command(
            &fixture,
            command,
            None,
            "warm nested dependency baseline",
            &dependency_paths,
        );
        let original_event_source =
            std::fs::read(fixture.root().join("src/lib.rs")).expect("event source");
        fixture.write("wire-leaf/src/lib.rs", "pub type Label = u32;");
        frozen_command(
            &fixture,
            command,
            Some(diagnostic),
            "warm dependency-only alias drift",
            &dependency_paths,
        );
        fixture.write(
            "wire-leaf/src/lib.rs",
            "type Renamed = String; pub type Label = Renamed;",
        );
        frozen_command(
            &fixture,
            command,
            None,
            "warm wire-equivalent dependency alias",
            &dependency_paths,
        );
        assert_eq!(
            std::fs::read(fixture.root().join("src/lib.rs")).expect("event source"),
            original_event_source,
            "dependency-only test must never change the aggregate or callback source"
        );
    }
}

fn nested_dependency_fixture(input: &str, leaf: &str) -> Fixture {
    let fixture = fixture(input);
    let manifest = std::fs::read_to_string(fixture.root().join("Cargo.toml")).expect("manifest");
    fixture.write(
        "Cargo.toml",
        &manifest.replacen(
            "[dependencies]",
            "[dependencies]\nwire-types = { path = \"wire-types\" }",
            1,
        ),
    );
    fixture.write("wire-types/Cargo.toml", "[package]\nname = \"wire-types\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[dependencies]\nwire-leaf = { path = \"../wire-leaf\" }\n");
    fixture.write(
        "wire-types/src/lib.rs",
        "pub type Label = wire_leaf::Label;\n",
    );
    fixture.write(
        "wire-leaf/Cargo.toml",
        "[package]\nname = \"wire-leaf\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    );
    fixture.write("wire-leaf/src/lib.rs", leaf);
    assert_success(&fixture.cargo(&["generate-lockfile", "--offline"]));
    fixture
}
