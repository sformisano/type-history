use super::super::support::{failure, success, LEDGER};
use super::{check, fixture, freeze, frozen_failure, replace, tests};

const CONTAINERS: &str = include_str!("../fixtures/fields/containers.rs");
const PRESENCE: &str = include_str!("../fixtures/fields/presence.rs");

#[test]
fn field_support_maps_sets_tuples_and_wrappers_keep_exact_frozen_contracts() {
    let fixture = fixture(&["rc"], "");
    fixture.write("src/lib.rs", CONTAINERS);
    tests(&fixture);
    freeze(&fixture);
    let ledger = fixture.read(LEDGER);
    let tree = replace(
        CONTAINERS,
        "pub type Map = HashMap<String, Option<Pair>>;",
        "pub type Map = BTreeMap<String, Option<Pair>>;",
    );
    let tree = replace(
        &tree,
        "pub type Members = HashSet<Exact>;",
        "pub type Members = BTreeSet<Exact>;",
    );
    fixture.write("src/lib.rs", &tree);
    check(&fixture);
    tests(&fixture);
    assert_eq!(fixture.read(LEDGER), ledger);

    let custom_hasher = replace(
        CONTAINERS,
        "pub type Map = HashMap<String, Option<Pair>>;",
        "pub type Map = HashMap<String, Option<Pair>, BuildHasherDefault<DefaultHasher>>;",
    );
    fixture.write("src/lib.rs", &custom_hasher);
    check(&fixture);
    tests(&fixture);

    // These are production build/check failures, before any data conversion.
    for (before, after) in [
        ("pub u32", "pub u64"),
        (
            "pub type Values = Vec<String>;",
            "pub type Values = BTreeSet<String>;",
        ),
        ("consumer:exact:v1", "consumer:ascii-folded:v1"),
        (
            "pub type Members = HashSet<Exact>;",
            "pub type Members = HashSet<Folded>;",
        ),
        (
            "pub type Position = (u32, String);",
            "pub type Position = (String, u32);",
        ),
        ("pub one: (u32,)", "pub one: u32"),
        ("pub pair: Pair", "pub pair: (u32, Option<String>)"),
    ] {
        let changed = replace(CONTAINERS, before, after);
        // A multi-field tuple struct has the same positional wire contract.
        if before == "pub pair: Pair" {
            let changed = replace(&changed, "pair: Pair(8, None)", "pair: (8, None)");
            fixture.write("src/lib.rs", &changed);
            check(&fixture);
            tests(&fixture);
        } else {
            frozen_failure(&fixture, &changed, "changed its frozen wire shape");
        }
    }
    fixture.write("src/lib.rs", CONTAINERS);
    check(&fixture);
    tests(&fixture);

    let missing_membership = replace(CONTAINERS,
        "impl SetMembership for Exact {\n    const MEMBERSHIP: ConstantMembership = ConstantMembership::custom(\"consumer:exact:v1\");\n}", "");
    fixture.write("src/lib.rs", &missing_membership);
    failure(
        &fixture.cargo(&["check", "--locked", "--offline"]),
        "SetMembership",
    );
    assert_eq!(fixture.read(LEDGER), ledger);
}

#[test]
fn field_support_presence_omission_null_and_nested_options() {
    let fixture = fixture(&["rc"], "");
    fixture.write("src/lib.rs", PRESENCE);
    tests(&fixture);
    freeze(&fixture);
    tests(&fixture);
    for field in [
        "Option<Option<u32>>",
        "Option<Box<Option<u32>>>",
        "Option<Rc<Option<u32>>>",
        "Option<Arc<Option<u32>>>",
        "Option<Nullable>",
        "Option<Nested>",
        "Option<Box<Nullable>>",
        "Option<Rc<Nullable>>",
        "Option<Arc<Nullable>>",
    ] {
        fixture.write(
            "src/lib.rs",
            &format!("{PRESENCE}\n#[derive(Schema)] struct Bad {{ value: {field} }}\n"),
        );
        failure(
            &fixture.cargo(&["check", "--locked", "--offline"]),
            "NonOptionalNode",
        );
    }
}

#[test]
fn field_support_sets_and_presence_require_explicit_versioned_migrations() {
    let fixture = fixture(&[], "");
    let initial = include_str!("../fixtures/fields/migrations_v1.rs");
    fixture.write("src/lib.rs", initial);
    tests(&fixture);
    freeze(&fixture);
    for (before, after) in [
        ("pub items: Vec<String>", "pub items: BTreeSet<String>"),
        ("pub value: Option<u32>", "pub value: Nullable"),
        ("pub required: Nullable", "pub required: Option<u32>"),
    ] {
        frozen_failure(
            &fixture,
            &replace(initial, before, after),
            "changed its frozen wire shape",
        );
    }
    fixture.write(
        "src/lib.rs",
        include_str!("../fixtures/fields/migrations_v2.rs"),
    );
    tests(&fixture);
    freeze(&fixture);
    check(&fixture);
    tests(&fixture);
    success(&fixture.cargo(&["check", "--release", "--locked", "--offline"]));
}
