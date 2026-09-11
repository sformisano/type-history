//! Discovery preserves Rust path roots and rejects conditional history selection.

use super::support::{failure, success, Fixture, LEDGER, V1};

#[test]
fn absolute_facade_paths_preserve_frozen_history_with_local_name_collisions() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    for source in [
        V1.replace("use history_api::versioned;", "mod history_api {}")
            .replace("#[versioned", "#[::history_api::versioned"),
        V1.replace(
            "use history_api::versioned;",
            "mod history_api {} use ::history_api::versioned;",
        ),
        V1.replace(
            "use history_api::versioned;",
            "#[cfg(any())] mod history_api {} use ::history_api::versioned;",
        ),
    ] {
        fixture.write("src/lib.rs", &source);
        success(&fixture.cargo(&["build", "--locked", "--offline"]));
        success(&fixture.cli(&["check"]));
        assert_eq!(fixture.read(LEDGER), frozen);
    }
}

#[test]
fn unrelated_platform_imports_preserve_frozen_history() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    fixture.write(
        "src/lib.rs",
        &format!(
            r#"
#[cfg(unix)]
mod platform {{ pub struct Thing; }}
#[cfg(not(unix))]
mod platform {{ pub struct Thing; }}
use platform::Thing;

pub fn platform_value() -> Thing {{ Thing }}

{V1}
"#
        ),
    );
    success(&fixture.cargo(&["build", "--locked", "--offline"]));
    success(&fixture.cli(&["check"]));
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    assert_eq!(fixture.read(LEDGER), frozen);
}

#[test]
fn conditional_facade_aliases_cannot_hide_frozen_history() {
    let fixture = Fixture::frozen();
    let frozen = fixture.read(LEDGER);
    for imports in [
        "#[cfg(any())] mod history_api {} use history_api::versioned;",
        "#[cfg(any())] mod history_api {} use history_api as api; use api::versioned;",
        "#[cfg(any())] use another_api as history_api; use history_api::versioned;",
        "#[cfg(any())] use history_api::versioned;",
    ] {
        fixture.write(
            "src/lib.rs",
            &V1.replace("use history_api::versioned;", imports),
        );
        failure(
            &fixture.cargo(&["build", "--locked", "--offline"]),
            "conditional",
        );
        failure(&fixture.cli(&["check"]), "conditional");
        assert_eq!(fixture.read(LEDGER), frozen);
    }
}
