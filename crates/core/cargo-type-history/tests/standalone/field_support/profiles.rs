use super::{check, fixture, freeze, frozen_failure, replace, tests};

const FLOATS: &str = include_str!("../fixtures/fields/floats.rs");
const LOGICAL: &str = include_str!("../fixtures/fields/logical.rs");
const TEMPORAL: &str = include_str!("../fixtures/fields/temporal.rs");

#[test]
fn field_support_floats_preserve_bits_and_reject_width_changes() {
    let fixture = fixture(&["typed-floats"], "float_native = { package = \"typed_floats\", version = \"1.0.7\", features = [\"serde\"] }");
    fixture.write("src/lib.rs", FLOATS);
    tests(&fixture);
    freeze(&fixture);
    tests(&fixture);
    frozen_failure(
        &fixture,
        &replace(
            FLOATS,
            "pub type Finite32 = NonNaNFinite<f32>;",
            "pub type Finite32 = NonNaNFinite<f64>;",
        ),
        "changed its frozen wire shape",
    );
}

#[test]
fn field_support_uuid_decimal_own_their_profiles() {
    let fixture = fixture(
        &["uuid", "rust-decimal"],
        r#"
uuid_native = { package = "uuid", version = "1.26.1" }
decimal_native = { package = "rust_decimal", version = "1.43.0", default-features = false, features = ["std"] }
"#,
    );
    fixture.write("src/lib.rs", LOGICAL);
    tests(&fixture);
    freeze(&fixture);
    for (before, after) in [
        (
            "pub type Identifier = UuidText;",
            "pub type Identifier = String;",
        ),
        ("pub type Money = DecimalText;", "pub type Money = String;"),
    ] {
        frozen_failure(
            &fixture,
            &replace(LOGICAL, before, after),
            "changed its frozen wire shape",
        );
    }
    fixture.write("src/lib.rs", LOGICAL);
    check(&fixture);
    tests(&fixture);
}

#[test]
fn field_support_temporal_profiles_cross_read_both_libraries() {
    let fixture = fixture(
        &["chrono", "time"],
        r#"
chrono_native = { package = "chrono", version = "0.4.45", default-features = false, features = ["std"] }
time_native = { package = "time", version = "0.3.55" }
"#,
    );
    fixture.write("src/lib.rs", TEMPORAL);
    tests(&fixture);
    freeze(&fixture);
    let time = replace(
        TEMPORAL,
        "use history_api::adapters::chrono as temporal;",
        "use history_api::adapters::time as temporal;",
    );
    fixture.write("src/lib.rs", &time);
    check(&fixture);
    tests(&fixture);
    // Re-read the Time-written reverse fixture through Chrono too.
    fixture.write("src/lib.rs", TEMPORAL);
    tests(&fixture);
    for (before, after) in [
        ("pub date: Date", "pub date: LocalTime"),
        ("pub utc: UtcInstant", "pub utc: OffsetDateTime"),
    ] {
        frozen_failure(
            &fixture,
            &replace(TEMPORAL, before, after),
            "changed its frozen wire shape",
        );
    }
}
