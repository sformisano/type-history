use super::{
    parse_field_history, parse_retained_floor, FieldTransition, HistoryPlan, ReconstructedHistory,
};
use crate::model::NamedField;
use quote::format_ident;
use syn::{parse_quote, Attribute, Ident, Type};

fn field(attributes: Vec<Attribute>, name: &str, ty: Type) -> NamedField {
    NamedField {
        attributes,
        name: format_ident!("{name}"),
        ty,
    }
}

fn opened() -> Ident {
    format_ident!("Opened")
}

#[test]
fn frozen_inventory_rejects_an_update_that_invents_a_predecessor_field() {
    use crate::ledger::{HistoryLedger, RecordMetadata, SchemaIdentity};
    let identity = "fixture.aggregates.account.events.opened";
    let baseline = HistoryLedger::<RecordMetadata>::parse(format!(
        r#"{{"{identity}":{{"1":{{"metadata":{{}},"schema":{{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"urn:typehistory:schema:{identity}","type":"object","additionalProperties":false,"properties":{{}},"required":[]}}}}}}}}"#
    ).as_bytes(), SchemaIdentity::new("urn:typehistory:schema:")).expect("empty frozen V1");
    let fields = vec![field(
        vec![
            parse_quote!(#[history(updated_at = v2, previous_type = String, cast_from_previous_value = convert)]),
        ],
        "owner",
        parse_quote!(String),
    )];
    let plan = HistoryPlan::infer(&opened(), None, &fields).unwrap();
    let Err(error) =
        plan.expand_authorized(&opened(), &fields, identity, &baseline, &RecordMetadata {})
    else {
        panic!("an update cannot invent a frozen predecessor field");
    };
    assert!(error.to_string().contains("added_at"), "{error}");
}

#[test]
fn duplicate_raw_field_names_and_zero_floor_fail_before_expansion() {
    let fields = vec![
        field(vec![], "name", parse_quote!(String)),
        field(vec![], "r#name", parse_quote!(String)),
    ];
    assert!(HistoryPlan::infer(&opened(), None, &fields).is_err());
    assert!(HistoryPlan::infer(&opened(), Some(0), &[]).is_err());
}

fn type_text(history: &ReconstructedHistory, version_index: usize, field_index: usize) -> String {
    let ty = &history.versions[version_index].fields[field_index].ty;
    quote::quote!(#ty).to_string()
}

#[test]
fn an_event_without_history_is_one_version() {
    let fields = vec![field(Vec::new(), "owner_name", parse_quote!(OwnerName))];
    let history = ReconstructedHistory::reconstruct(&opened(), None, &fields).expect("reconstruct");
    assert_eq!(history.floor, 1);
    assert_eq!(history.head, 1);
    assert_eq!(history.versions.len(), 1);
    assert!(history.transitions.is_empty());
}

#[test]
fn an_empty_payload_is_version_one() {
    let history = ReconstructedHistory::reconstruct(&opened(), None, &[]).expect("reconstruct");
    assert_eq!(history.head, 1);
    assert_eq!(history.versions.len(), 1);
    assert!(history.versions[0].fields.is_empty());
}

#[test]
fn boundaries_infer_the_current_version() {
    let fields = vec![
        field(
            vec![
                parse_quote!(#[history(updated_at = v2, previous_type = LegacyOwnerName, cast_from_previous_value = owner_name_from_history)]),
            ],
            "owner_name",
            parse_quote!(OwnerName),
        ),
        field(
            vec![parse_quote!(#[history(added_at = v3, backfill_value = "und".to_owned())])],
            "preferred_locale",
            parse_quote!(String),
        ),
    ];
    let history = ReconstructedHistory::reconstruct(&opened(), None, &fields).expect("reconstruct");
    assert_eq!(history.head, 3);
    assert_eq!(history.retained_versions(), vec![1, 2, 3]);
    assert_eq!(type_text(&history, 0, 0), "LegacyOwnerName");
    assert_eq!(type_text(&history, 1, 0), "OwnerName");
    assert_eq!(history.versions[1].fields.len(), 1);
    assert_eq!(history.versions[2].fields.len(), 2);
    assert_eq!(history.transitions.len(), 2);
}

#[test]
fn repeated_updates_reconstruct_each_predecessor_type() {
    let fields = vec![field(
        vec![
            parse_quote!(#[history(removed_at = v6)]),
            parse_quote!(#[history(updated_at = v5, previous_type = u32, cast_from_previous_value = widen_count_u32)]),
            parse_quote!(#[history(updated_at = v3, previous_type = u16, cast_from_previous_value = widen_count_u16)]),
            parse_quote!(#[history(added_at = v2, backfill_value = 0_u16)]),
        ],
        "count",
        parse_quote!(u64),
    )];
    let history = ReconstructedHistory::reconstruct(&opened(), None, &fields).expect("reconstruct");
    assert_eq!(history.head, 6);
    assert!(history.versions[0].fields.is_empty(), "absent at V1");
    assert_eq!(type_text(&history, 1, 0), "u16", "u16 at V2");
    assert_eq!(type_text(&history, 2, 0), "u32", "u32 at V3");
    assert_eq!(type_text(&history, 3, 0), "u32", "u32 at V4");
    assert_eq!(type_text(&history, 4, 0), "u64", "u64 at V5");
    assert!(history.versions[5].fields.is_empty(), "absent from V6");
}

#[test]
fn a_reversed_record_order_is_rejected() {
    let fields = vec![field(
        vec![
            parse_quote!(#[history(added_at = v2, backfill_value = 0_u16)]),
            parse_quote!(#[history(updated_at = v3, previous_type = u16, cast_from_previous_value = widen)]),
        ],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), None, &fields)
        .err()
        .expect("order");
    assert!(error.to_string().contains("newest first"), "{error}");
}

#[test]
fn an_addition_without_an_initializer_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_at = v2)]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("initializer");
    assert!(
        error.to_string().contains("requires `backfill_value`"),
        "{error}"
    );
}

#[test]
fn an_update_without_a_previous_type_is_rejected() {
    let attribute: Attribute =
        parse_quote!(#[history(updated_at = v2, cast_from_previous_value = convert)]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("previous type");
    assert!(
        error.to_string().contains("`previous_type = PreviousType`"),
        "{error}"
    );
}

#[test]
fn the_old_from_key_is_not_a_previous_type_alias() {
    let attribute: Attribute = parse_quote!(#[history(
        updated_at = v2,
        from = String,
        cast_from_previous_value = convert,
    )]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("unknown key");
    assert!(
        error.to_string().contains("unknown history key `from`"),
        "{error}"
    );
}

#[test]
fn competing_assignment_rules_are_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_at = v2, backfill_value = 0, cast_from_previous_payload = build)]);
    let error = parse_field_history(&[attribute]).err().expect("conflict");
    assert!(
        error.to_string().contains("exactly one assignment rule"),
        "{error}"
    );
}

#[test]
fn a_previous_value_callback_on_an_addition_is_rejected() {
    let attribute: Attribute =
        parse_quote!(#[history(added_at = v2, cast_from_previous_value = convert)]);
    let error = parse_field_history(&[attribute]).err().expect("addition");
    assert!(
        error.to_string().contains("no predecessor value"),
        "{error}"
    );
}

#[test]
fn a_removal_rejects_a_callback() {
    let attribute: Attribute =
        parse_quote!(#[history(removed_at = v2, cast_from_previous_value = convert)]);
    let error = parse_field_history(&[attribute]).err().expect("removal");
    assert!(
        error.to_string().contains("carries no conversion"),
        "{error}"
    );
}

#[test]
fn a_boundary_at_or_below_the_floor_is_rejected() {
    let fields = vec![field(
        vec![parse_quote!(#[history(added_at = v3, backfill_value = 0_u32)])],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), Some(3), &fields)
        .err()
        .expect("floor");
    assert!(
        error.to_string().contains("not above the retained floor"),
        "{error}"
    );
}

#[test]
fn an_imported_floor_retains_only_its_real_history() {
    let fields = vec![field(
        vec![parse_quote!(#[history(added_at = v4, backfill_value = 0_u32)])],
        "count",
        parse_quote!(u32),
    )];
    let history =
        ReconstructedHistory::reconstruct(&opened(), Some(3), &fields).expect("reconstruct");
    assert_eq!(history.floor, 3);
    assert_eq!(history.head, 4);
    assert_eq!(history.retained_versions(), vec![3, 4]);
}

#[test]
fn an_imported_floor_without_a_boundary_infers_that_floor() {
    let fields = vec![field(Vec::new(), "count", parse_quote!(u32))];
    let history =
        ReconstructedHistory::reconstruct(&opened(), Some(3), &fields).expect("reconstruct");
    assert_eq!(history.floor, 3);
    assert_eq!(history.head, 3);
    assert_eq!(history.retained_versions(), vec![3]);
}

#[test]
fn a_removal_still_contributes_to_inference_with_an_empty_latest_payload() {
    let fields = vec![field(
        vec![parse_quote!(#[history(removed_at = v2)])],
        "count",
        parse_quote!(u32),
    )];
    let history = ReconstructedHistory::reconstruct(&opened(), None, &fields).expect("reconstruct");
    assert_eq!(history.head, 2);
    assert_eq!(history.versions[1].fields.len(), 0);
}

#[test]
fn an_identity_step_is_generated_for_an_unchanged_intermediate_version() {
    let fields = vec![
        field(
            vec![parse_quote!(#[history(added_at = v2, backfill_value = 0_u32)])],
            "first",
            parse_quote!(u32),
        ),
        field(
            vec![parse_quote!(#[history(added_at = v4, backfill_value = 0_u32)])],
            "second",
            parse_quote!(u32),
        ),
    ];
    let history = ReconstructedHistory::reconstruct(&opened(), None, &fields).expect("reconstruct");
    assert_eq!(history.head, 4);
    let identity = &history.transitions[1];
    assert_eq!((identity.from, identity.to), (2, 3));
    assert_eq!(identity.fields.len(), 1);
    assert!(matches!(identity.fields[0], FieldTransition::Carry { .. }));
}

#[test]
fn a_version_zero_boundary_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_at = v0, backfill_value = 0)]);
    let error = parse_field_history(&[attribute]).err().expect("v0");
    assert!(error.to_string().contains("positive"), "{error}");
}

#[test]
fn a_boundary_above_the_supported_range_is_rejected() {
    let attribute: Attribute =
        parse_quote!(#[history(added_at = v99999999999, backfill_value = 0)]);
    let error = parse_field_history(&[attribute]).err().expect("range");
    assert!(
        error.to_string().contains("supported positive range"),
        "{error}"
    );
}

#[test]
fn an_unknown_history_key_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_at = v2, initial = 0)]);
    let error = parse_field_history(&[attribute]).err().expect("unknown");
    assert!(error.to_string().contains("unknown history key"), "{error}");
}

#[test]
fn an_unknown_history_record_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(current = v2)]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("unknown record");
    assert!(
        error.to_string().contains("unknown history record"),
        "{error}"
    );
}

#[test]
fn a_retained_floor_on_a_field_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(retained_from = v3)]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("floor placement");
    assert!(
        error.to_string().contains("place it on the event"),
        "{error}"
    );
}

#[test]
fn an_event_level_field_record_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_at = v2, backfill_value = 0)]);
    let error = parse_retained_floor(&[attribute]).expect_err("record placement");
    assert!(
        error.to_string().contains("place it on the field"),
        "{error}"
    );
}

#[test]
fn a_duplicate_boundary_on_one_field_is_rejected() {
    let fields = vec![field(
        vec![
            parse_quote!(#[history(updated_at = v2, previous_type = u16, cast_from_previous_value = a)]),
            parse_quote!(#[history(updated_at = v2, previous_type = u8, cast_from_previous_value = b)]),
        ],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), None, &fields)
        .err()
        .expect("duplicate");
    assert!(error.to_string().contains("distinct versions"), "{error}");
}

#[test]
fn an_update_after_a_removal_is_rejected() {
    let fields = vec![field(
        vec![
            parse_quote!(#[history(updated_at = v4, previous_type = u16, cast_from_previous_value = a)]),
            parse_quote!(#[history(removed_at = v3)]),
        ],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), None, &fields)
        .err()
        .expect("after removal");
    assert!(error.to_string().contains("after its removal"), "{error}");
}
