use std::collections::BTreeMap;

use quote::ToTokens;
use syn::{parse_quote, Data, Fields, File, Item};
use type_history_core::resolved::{FieldPresence, SchemaField, SchemaShape};

use super::{
    generate_history, generate_history_with_options, GenerationOptions, GenerationPaths,
    PayloadTraits,
};
use crate::{
    history::HistoryPlan,
    json_schema::JsonSchemaDocument,
    ledger::{HistoryLedger, RecordMetadata, SchemaIdentity, Snapshot},
    NamedField, RecordDerives, RecordInput,
};

fn input() -> RecordInput {
    RecordInput {
        name: parse_quote!(Changed),
        visibility: parse_quote!(pub),
        attributes: vec![],
        derives: RecordDerives::default(),
        fields: vec![NamedField {
            name: parse_quote!(secret),
            ty: parse_quote!(String),
            attributes: vec![
                parse_quote!(#[allow(deprecated)]),
                parse_quote!(#[sensitive(mask = redact)]),
            ],
        }],
        field_visibility: BTreeMap::from([("secret".to_owned(), parse_quote!(pub(crate)))]),
    }
}

fn paths() -> GenerationPaths {
    GenerationPaths {
        support: parse_quote!(support),
        error: parse_quote!(Failure),
        helper_prefix: "changed_history".to_owned(),
    }
}

fn ledger(frozen: bool) -> HistoryLedger<RecordMetadata> {
    let identity = SchemaIdentity::new("urn:typehistory:schema:");
    if !frozen {
        return HistoryLedger::empty(identity);
    }
    let shape = SchemaShape::Record {
        fields: vec![SchemaField {
            name: "secret".to_owned(),
            presence: FieldPresence::Required,
            schema: SchemaShape::String,
        }],
    };
    HistoryLedger::from_entries(
        BTreeMap::from([(
            "fixture.changed".to_owned(),
            BTreeMap::from([(
                1,
                Snapshot {
                    metadata: RecordMetadata {},
                    schema: JsonSchemaDocument::from_shape(
                        &shape,
                        "urn:typehistory:schema:fixture.changed",
                    ),
                    reset_draft: false,
                },
            )]),
        )]),
        identity,
    )
    .unwrap()
}

#[test]
fn frontend_contract_keeps_alias_types_visibility_and_authored_policies() {
    let input = input();
    let history = HistoryPlan::infer(&input.name, &input.fields)
        .unwrap()
        .expand_authorized(
            &input.name,
            &input.fields,
            "fixture.changed",
            &ledger(false),
            &RecordMetadata {},
        )
        .unwrap();
    let generated = generate_history_with_options(
        &input,
        "fixture.changed",
        &history,
        &paths(),
        &GenerationOptions {
            payload_traits: PayloadTraits::Frontend,
            observation_cfg: None,
        },
    )
    .unwrap();
    let contract = &generated.versions[0].contract;
    let Data::Struct(data) = &contract.data else {
        panic!("record contract")
    };
    let Fields::Named(fields) = &data.fields else {
        panic!("named contract")
    };
    let field = fields.named.first().unwrap();
    assert_eq!(field.vis, parse_quote!(pub(crate)));
    assert!(field
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("sensitive")));
    let file = syn::parse2::<File>(generated.items).unwrap();
    let emitted = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(item) if item.ident == contract.ident => Some(item),
            _ => None,
        })
        .unwrap();
    assert_eq!(emitted.fields.iter().next().unwrap().ty, field.ty);
    assert!(emitted
        .attrs
        .iter()
        .all(|attr| !attr.path().is_ident("derive")));
    assert!(emitted
        .fields
        .iter()
        .next()
        .unwrap()
        .attrs
        .iter()
        .all(|attr| !attr.path().is_ident("sensitive")));
    assert!(
        !field.ty.to_token_stream().to_string().eq("String"),
        "lint-scoped alias must be retained"
    );
    assert!(generated.versions[0]
        .shape_expression
        .to_string()
        .contains("resolved_wire_schema"));
    assert!(generated.versions[0]
        .schema_expression
        .to_string()
        .contains("export_json_schema"));
}

#[test]
fn default_generation_keeps_native_traits_and_unconditional_frozen_guards() {
    let input = input();
    let history = HistoryPlan::infer(&input.name, &input.fields)
        .unwrap()
        .expand_authorized(
            &input.name,
            &input.fields,
            "fixture.changed",
            &ledger(true),
            &RecordMetadata {},
        )
        .unwrap();
    let generated = generate_history(&input, "fixture.changed", &history, &paths()).unwrap();
    let file = syn::parse2::<File>(generated.items).unwrap();
    let payload = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Struct(item) if item.ident == generated.latest => Some(item),
            _ => None,
        })
        .unwrap();
    let derives = payload
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("derive"))
        .unwrap()
        .to_token_stream()
        .to_string();
    for expected in ["Clone", "PartialEq", "Debug"] {
        assert!(derives.contains(expected));
    }
    let checks = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Const(item) => Some(item),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(checks.len(), 2);
    assert!(checks.iter().all(|item| item.attrs.is_empty()));
}

#[test]
fn observation_only_gates_frozen_assertions_under_test_and_the_selected_cfg() {
    let input = input();
    let history = HistoryPlan::infer(&input.name, &input.fields)
        .unwrap()
        .expand_authorized(
            &input.name,
            &input.fields,
            "fixture.changed",
            &ledger(true),
            &RecordMetadata {},
        )
        .unwrap();
    let generated = generate_history_with_options(
        &input,
        "fixture.changed",
        &history,
        &paths(),
        &GenerationOptions {
            observation_cfg: Some(parse_quote!(custom_frontend_export)),
            ..GenerationOptions::default()
        },
    )
    .unwrap();
    let file = syn::parse2::<File>(generated.items).unwrap();
    for item in file.items {
        if let Item::Const(item) = item {
            assert_eq!(
                item.attrs[0].to_token_stream().to_string(),
                "# [cfg (not (all (test , custom_frontend_export)))]"
            );
        }
    }
}
