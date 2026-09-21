//! Malformed histories must fail through the real domain build and macro.

use type_history_core::resolved::SchemaShape;

use super::super::support::{failure as assert_failure, success as assert_success};
use super::{fixture, ledger, source};

#[test]
fn history_grammar_rejects_invalid_lifetimes_before_expansion() {
    let initial = source(" count: u32,", "", "");
    let fixture = fixture(&initial);
    fixture.set_ledger(&ledger(vec![vec![("count", SchemaShape::U32)]]));
    assert_success(&fixture.cargo(&["check", "--locked", "--offline"]));
    for (attributes, expected) in [
        (
            "#[history::typo(added_in = v2, backfill_value = 0)]",
            "record fields support only documentation, lints, and `#[history(...)]`",
        ),
        ("#[history(added_in = v0, backfill_value = 0)]", "positive"),
        (
            "#[history(added_in = v4294967296, backfill_value = 0)]",
            "supported positive range",
        ),
        (
            "#[history(added_in = v4000000000, backfill_value = 0)]",
            "one permitted successor",
        ),
        (
            "#[history(added_in = v1, backfill_value = 0)]",
            "not above V1",
        ),
        ("#[history(added_in = v2)]", "requires exactly one"),
        (
            "#[history(added_in = v2, previous_type = u32, backfill_value = 0)]",
            "describes an update",
        ),
        (
            "#[history(added_at = v2, backfill_value = 0)]",
            "unknown history record",
        ),
        (
            "#[history(added_in = v2, backfill_value = 0, backfill_fn = cast)]",
            "exactly one",
        ),
        (
            "#[history(updated_in = v2, backfill_fn = cast)]",
            "requires `previous_type",
        ),
        (
            "#[history(updated_in = v2, from = u32, backfill_fn = cast)]",
            "unknown history key `from`",
        ),
        (
            "#[history(updated_in = v2, previous_type = u32, previous_type = u32, backfill_fn = cast)]",
            "duplicate history key `previous_type`",
        ),
        (
            "#[history(updated_in = v2, previous_type = u32)]",
            "requires exactly one",
        ),
        (
            "#[history(updated_in = v2, previous_type = u32, cast_from_previous_value = cast)]",
            "unknown history key",
        ),
        (
            "#[history(updated_in = v2, previous_type = u32, backfill_fn = cast, backfill_fn = cast)]",
            "duplicate history key",
        ),
        (
            "#[history(removed_in = v2, previous_type = u32)]",
            "removal carries no backfill",
        ),
        (
            "#[history(removed_in = v2, backfill_value = 0)]",
            "removal carries no backfill",
        ),
        (
            "#[history(added_in = v2, backfill_value = 0, backfill_value = 1)]",
            "duplicate history key",
        ),
        (
            "#[history(added_in = v2, unknown = 0)]",
            "unknown history key",
        ),
        ("#[history(current = v2)]", "unknown history record"),
        ("#[history(retained_from = v2)]", "unknown history record"),
        (
            "#[history(removed_in = v2)] #[history(removed_in = v3)]",
            "newest first",
        ),
        (
            "#[history(removed_in = v3)] #[history(removed_in = v2)]",
            "more than one removal",
        ),
        (
            "#[history(added_in = v3, backfill_value = 0)] #[history(added_in = v2, backfill_value = 0)]",
            "more than one birth",
        ),
        (
            "#[history(updated_in = v2, previous_type = u32, backfill_fn = cast)] #[history(added_in = v2, backfill_value = 0)]",
            "distinct versions",
        ),
        (
            "#[history(updated_in = v3, previous_type = u32, backfill_fn = cast)] #[history(removed_in = v2)]",
            "after its removal",
        ),
    ] {
        fixture.write(
            "src/lib.rs",
            &source(&format!(" {attributes} count: u32,"), "", ""),
        );
        eprintln!("history grammar: {attributes}");
        assert_failure(
            &fixture.cargo(&["check", "--locked", "--offline"]),
            expected,
        );
    }
    fixture.write("src/lib.rs", &source(" count: u32,  r#count: u32,", "", ""));
    assert_failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        "duplicate",
    );
    fixture.write("src/lib.rs", &source(" count: u32,  #[history(updated_in = v2, previous_type = u32, backfill_fn = cast)] invented: u32,", "", ""));
    assert_failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        "added_in",
    );
    fixture.write("src/lib.rs", "");
    assert_failure(
        &fixture.cargo(&["build", "--locked", "--offline"]),
        "retained",
    );
    fixture.write("src/lib.rs", &initial);
    assert_success(&fixture.cargo(&["build", "--locked", "--offline"]));
}

#[test]
fn history_grammar_rejects_unbounded_heads_and_non_v1_ledgers() {
    const ID: &str = "billing.invoice.issued";
    let baseline = ledger(vec![vec![("count", SchemaShape::U32)]]);
    let input = source(" count: u32,", "", "");
    let fixture = fixture(&input);
    fixture.set_ledger(&serde_json::json!({ ID: { "4294967295": baseline[ID]["1"].clone() } }));
    assert_failure(&fixture.cargo(&["check", "--locked", "--offline"]), "V1");
    fixture.set_ledger(&baseline);
    for (boundary, diagnostic) in [
        ("v4294967295", "one permitted successor"),
        ("v4294967296", "supported positive range"),
    ] {
        fixture.write(
            "src/lib.rs",
            &source(
                &format!(" #[history(added_in = {boundary}, backfill_value = 0)] count: u32,"),
                "",
                "",
            ),
        );
        assert_failure(
            &fixture.cargo(&["check", "--locked", "--offline"]),
            diagnostic,
        );
    }
}
