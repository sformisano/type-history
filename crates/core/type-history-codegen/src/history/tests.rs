use super::{parse_field_history, FieldTransition, HistoryPlan, ReconstructedHistory};
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
    let stable_name = "fixture.account.opened";
    let baseline = HistoryLedger::<RecordMetadata>::parse(format!(
        r#"{{"{stable_name}":{{"1":{{"metadata":{{}},"schema":{{"$schema":"https://json-schema.org/draft/2020-12/schema","$id":"urn:typehistory:schema:{stable_name}","type":"object","additionalProperties":false,"properties":{{}},"required":[]}}}}}}}}"#
    ).as_bytes(), SchemaIdentity::new("urn:typehistory:schema:")).expect("empty frozen V1");
    let fields = vec![field(
        vec![
            parse_quote!(#[history(updated_in = v2, previous_type = String, backfill_fn = convert)]),
        ],
        "owner",
        parse_quote!(String),
    )];
    let plan = HistoryPlan::infer(&opened(), &fields).unwrap();
    let Err(error) = plan.expand_authorized(
        &opened(),
        &fields,
        stable_name,
        &baseline,
        &RecordMetadata {},
    ) else {
        panic!("an update cannot invent a frozen predecessor field");
    };
    assert!(error.to_string().contains("added_in"), "{error}");
}

#[test]
fn duplicate_raw_field_names_fail_before_expansion() {
    let fields = vec![
        field(vec![], "name", parse_quote!(String)),
        field(vec![], "r#name", parse_quote!(String)),
    ];
    assert!(HistoryPlan::infer(&opened(), &fields).is_err());
}

fn type_text(history: &ReconstructedHistory, version_index: usize, field_index: usize) -> String {
    let ty = &history.versions[version_index].fields[field_index].ty;
    quote::quote!(#ty).to_string()
}

#[test]
fn an_event_without_history_is_one_version() {
    let fields = vec![field(Vec::new(), "owner_name", parse_quote!(OwnerName))];
    let history = ReconstructedHistory::reconstruct(&opened(), &fields).expect("reconstruct");
    assert_eq!(history.retained_versions(), vec![1]);
    assert_eq!(history.head, 1);
    assert_eq!(history.versions.len(), 1);
    assert!(history.transitions.is_empty());
}

#[test]
fn an_empty_payload_is_version_one() {
    let history = ReconstructedHistory::reconstruct(&opened(), &[]).expect("reconstruct");
    assert_eq!(history.head, 1);
    assert_eq!(history.versions.len(), 1);
    assert!(history.versions[0].fields.is_empty());
}

#[test]
fn boundaries_infer_the_current_version() {
    let fields = vec![
        field(
            vec![
                parse_quote!(#[history(updated_in = v2, previous_type = LegacyOwnerName, backfill_fn = owner_name_from_history)]),
            ],
            "owner_name",
            parse_quote!(OwnerName),
        ),
        field(
            vec![parse_quote!(#[history(added_in = v3, backfill_value = "und".to_owned())])],
            "preferred_locale",
            parse_quote!(String),
        ),
    ];
    let history = ReconstructedHistory::reconstruct(&opened(), &fields).expect("reconstruct");
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
            parse_quote!(#[history(removed_in = v6)]),
            parse_quote!(#[history(updated_in = v5, previous_type = u32, backfill_fn = widen_count_u32)]),
            parse_quote!(#[history(updated_in = v3, previous_type = u16, backfill_fn = widen_count_u16)]),
            parse_quote!(#[history(added_in = v2, backfill_value = 0_u16)]),
        ],
        "count",
        parse_quote!(u64),
    )];
    let history = ReconstructedHistory::reconstruct(&opened(), &fields).expect("reconstruct");
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
            parse_quote!(#[history(added_in = v2, backfill_value = 0_u16)]),
            parse_quote!(#[history(updated_in = v3, previous_type = u16, backfill_fn = widen)]),
        ],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), &fields)
        .err()
        .expect("order");
    assert!(error.to_string().contains("newest first"), "{error}");
}

#[test]
fn an_addition_without_an_initializer_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_in = v2)]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("initializer");
    assert!(
        error
            .to_string()
            .contains("requires exactly one of `backfill_value`"),
        "{error}"
    );
}

#[test]
fn an_update_without_a_previous_type_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(updated_in = v2, backfill_fn = convert)]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("previous type");
    assert!(
        error.to_string().contains("`previous_type = PreviousType`"),
        "{error}"
    );
}

#[test]
fn unknown_update_keys_are_rejected() {
    let attribute: Attribute = parse_quote!(#[history(
        updated_in = v2,
        source_type = String,
        backfill_fn = convert,
    )]);
    let error = parse_field_history(&[attribute])
        .err()
        .expect("unknown key");
    assert!(
        error
            .to_string()
            .contains("unknown history key `source_type`"),
        "{error}"
    );
}

#[test]
fn competing_assignment_rules_are_rejected() {
    let attribute: Attribute =
        parse_quote!(#[history(added_in = v2, backfill_value = 0, backfill_fn = build)]);
    let error = parse_field_history(&[attribute]).err().expect("conflict");
    assert!(error.to_string().contains("exactly one of"), "{error}");
}

#[test]
fn duplicate_backfill_functions_are_rejected() {
    let attribute: Attribute =
        parse_quote!(#[history(added_in = v2, backfill_fn = first, backfill_fn = second)]);
    let error = parse_field_history(&[attribute]).err().expect("duplicate");
    assert!(
        error
            .to_string()
            .contains("duplicate history key `backfill_fn`"),
        "{error}"
    );
}

#[test]
fn a_removal_rejects_a_callback() {
    let attribute: Attribute = parse_quote!(#[history(removed_in = v2, backfill_fn = convert)]);
    let error = parse_field_history(&[attribute]).err().expect("removal");
    assert!(error.to_string().contains("carries no backfill"), "{error}");
}

#[test]
fn a_boundary_at_version_one_is_rejected() {
    let fields = vec![field(
        vec![parse_quote!(#[history(added_in = v1, backfill_value = 0_u32)])],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), &fields)
        .err()
        .expect("boundary");
    assert!(error.to_string().contains("not above V1"), "{error}");
}

#[test]
fn a_removal_still_contributes_to_inference_with_an_empty_latest_payload() {
    let fields = vec![field(
        vec![parse_quote!(#[history(removed_in = v2)])],
        "count",
        parse_quote!(u32),
    )];
    let history = ReconstructedHistory::reconstruct(&opened(), &fields).expect("reconstruct");
    assert_eq!(history.head, 2);
    assert_eq!(history.versions[1].fields.len(), 0);
}

#[test]
fn an_identity_step_is_generated_for_an_unchanged_intermediate_version() {
    let fields = vec![
        field(
            vec![parse_quote!(#[history(added_in = v2, backfill_value = 0_u32)])],
            "first",
            parse_quote!(u32),
        ),
        field(
            vec![parse_quote!(#[history(added_in = v4, backfill_value = 0_u32)])],
            "second",
            parse_quote!(u32),
        ),
    ];
    let history = ReconstructedHistory::reconstruct(&opened(), &fields).expect("reconstruct");
    assert_eq!(history.head, 4);
    let identity = &history.transitions[1];
    assert_eq!((identity.from, identity.to), (2, 3));
    assert_eq!(identity.fields.len(), 1);
    assert!(matches!(identity.fields[0], FieldTransition::Carry { .. }));
}

#[test]
fn a_version_zero_boundary_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_in = v0, backfill_value = 0)]);
    let error = parse_field_history(&[attribute]).err().expect("v0");
    assert!(error.to_string().contains("positive"), "{error}");
}

#[test]
fn a_boundary_above_the_supported_range_is_rejected() {
    let attribute: Attribute =
        parse_quote!(#[history(added_in = v99999999999, backfill_value = 0)]);
    let error = parse_field_history(&[attribute]).err().expect("range");
    assert!(
        error.to_string().contains("supported positive range"),
        "{error}"
    );
}

#[test]
fn an_unknown_history_key_is_rejected() {
    let attribute: Attribute = parse_quote!(#[history(added_in = v2, initial = 0)]);
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
fn a_duplicate_boundary_on_one_field_is_rejected() {
    let fields = vec![field(
        vec![
            parse_quote!(#[history(updated_in = v2, previous_type = u16, backfill_fn = a)]),
            parse_quote!(#[history(updated_in = v2, previous_type = u8, backfill_fn = b)]),
        ],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), &fields)
        .err()
        .expect("duplicate");
    assert!(error.to_string().contains("distinct versions"), "{error}");
}

#[test]
fn an_update_after_a_removal_is_rejected() {
    let fields = vec![field(
        vec![
            parse_quote!(#[history(updated_in = v4, previous_type = u16, backfill_fn = a)]),
            parse_quote!(#[history(removed_in = v3)]),
        ],
        "count",
        parse_quote!(u32),
    )];
    let error = ReconstructedHistory::reconstruct(&opened(), &fields)
        .err()
        .expect("after removal");
    assert!(error.to_string().contains("after its removal"), "{error}");
}

#[test]
fn additions_and_updates_share_the_complete_backfill_grammar() {
    use syn::ItemStruct;
    for marker in ["added_in", "updated_in", "removed_in"] {
        for mask in 0..8 {
            let mut arguments = format!("{marker} = v2");
            for (bit, key) in [
                (1, "previous_type = u32"),
                (2, "backfill_value = 0"),
                (4, "backfill_fn = fill"),
            ] {
                if mask & bit != 0 {
                    arguments.push_str(&format!(", {key}"));
                }
            }
            let source = format!("struct Probe {{ #[history({arguments})] value: u64 }}");
            let item: ItemStruct = syn::parse_str(&source).unwrap();
            let attributes = &item.fields.iter().next().unwrap().attrs;
            let expected = match marker {
                "added_in" => mask == 2 || mask == 4,
                "updated_in" => mask == 3 || mask == 5,
                _ => mask == 0,
            };
            assert_eq!(
                parse_field_history(attributes).is_ok(),
                expected,
                "{arguments}"
            );
        }
    }
}
