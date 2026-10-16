//! Supported Cargo and Rust import layouts retain the same history authority.

use super::support;
use support::{failure, success, Fixture, LEDGER};

#[path = "source_discovery/cargo_inputs.rs"]
mod cargo_inputs;
#[path = "source_discovery/conditional.rs"]
mod conditional;
#[path = "source_discovery/imports.rs"]
mod imports;

fn reject_draft_and_freeze(fixture: &Fixture) {
    let empty = fixture.read(LEDGER);
    {
        let strict = "TYPE_HISTORY_REQUIRE_FROZEN";
        failure(
            &fixture.cargo_env(&["build", "--locked", "--offline"], &[(strict, Some("1"))]),
            "draft",
        );
    }
    failure(
        &fixture.cargo(&["build", "--release", "--locked", "--offline"]),
        "draft",
    );
    assert_eq!(fixture.read(LEDGER), empty);
    success(&fixture.cli(&["freeze", "--package", "standalone-history-consumer"]));
    success(&fixture.cli(&["check"]));
    success(&fixture.cargo(&["build", "--release", "--locked", "--offline"]));
    let frozen = fixture.read(LEDGER);
    fixture.write("src/lib.rs", "");
    failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "missing",
    );
    assert_eq!(fixture.read(LEDGER), frozen);
}
