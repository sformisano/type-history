use invoice_history::redaction::{
    AccountContacts, AccountCreated, AccountData, AccountDataV1, AccountDiagnostic,
    AccountDiagnosticV1, AccountMessage, AccountMessageV1, Contact, ContactChannel, PublicMessage,
    PublicRecord,
};
use redactable::ToRedacted;
use serde_json::{json, to_value};
use type_history::{decode, HasHistory, PayloadVersion, Versioned};

const OLD: &[u8] = br#"{"owner_name":"private-owner-canary","balance_minor":654321,"legacy_note":"private-note-canary"}"#;

#[test]
fn current_values_keep_raw_storage_and_redacted_diagnostics() {
    let account = AccountData {
        owner_name: "private-owner-canary".into(),
        balance_minor: 654_321,
        record_kind: "account".into(),
    };
    assert_eq!(
        to_value(&account).unwrap(),
        json!({
            "owner_name": "private-owner-canary", "balance_minor": 654321,
            "record_kind": "account"
        })
    );
    assert_eq!(
        account.to_redacted().json(),
        json!({
            "owner_name": "[REDACTED]", "balance_minor": 0, "record_kind": "account"
        })
    );
    assert_eq!(format!("{account:?}"), "AccountDataV2 { owner_name: \"[REDACTED]\", balance_minor: \"[REDACTED]\", record_kind: \"account\" }");
    assert_eq!(account.owner_name, "private-owner-canary");
    assert_eq!(account.balance_minor, 654_321);
    assert_eq!(AccountData::VERSION.get(), 2);
    let encoded = serde_json::to_vec(&account).unwrap();
    assert_eq!(
        decode::<AccountData>(&AccountData::STABLE_NAME, AccountData::VERSION, &encoded).unwrap(),
        account
    );
}

#[test]
fn removed_fields_remain_redacted_on_the_historical_type() {
    let old: AccountDataV1 = serde_json::from_slice(OLD).unwrap();
    assert_eq!(
        old.to_redacted().json(),
        json!({
            "owner_name": "[REDACTED]", "balance_minor": 0, "legacy_note": "[REDACTED]"
        })
    );
    assert_eq!(format!("{old:?}"), "AccountDataV1 { owner_name: \"[REDACTED]\", balance_minor: \"[REDACTED]\", legacy_note: \"[REDACTED]\" }");
    assert_eq!(
        to_value(&old).unwrap(),
        serde_json::from_slice::<serde_json::Value>(OLD).unwrap()
    );
}

#[test]
fn historical_conversion_keeps_business_values_and_current_redaction() {
    let current =
        decode::<AccountData>(&AccountData::STABLE_NAME, PayloadVersion::INITIAL, OLD).unwrap();
    assert_eq!(current.owner_name, "private-owner-canary");
    assert_eq!(current.balance_minor, 654_321_u128);
    assert_eq!(current.record_kind, "account");
    assert_eq!(
        current.to_redacted().json(),
        json!({
            "owner_name": "[REDACTED]", "balance_minor": 0, "record_kind": "account"
        })
    );
}

#[test]
fn historical_wrapper_preserves_the_original_payload_before_conversion() {
    let original = json!({
        "stable_name": "account.data", "version": 1,
        "payload": {"owner_name": "private-owner-canary", "balance_minor": 654321,
                    "legacy_note": "private-note-canary"}
    });
    let stored: Versioned<AccountData> = serde_json::from_value(original.clone()).unwrap();
    assert_eq!(stored.source_version().get(), 1);
    assert_eq!(to_value(&stored).unwrap(), original);
    let debug = format!("{stored:?}");
    assert!(debug.contains("account.data"));
    for private in ["private-owner-canary", "private-note-canary", "654321"] {
        assert!(!debug.contains(private));
    }
    let current = AccountData::from_versioned(stored).unwrap();
    assert_eq!(current.balance_minor, 654_321);
    assert_eq!(current.into_versioned().source_version().get(), 2);
}

#[test]
fn independently_persisted_event_uses_the_same_direct_derives() {
    let event = AccountCreated {
        owner_name: "private-owner-canary".into(),
    };
    assert_eq!(
        event.to_redacted().json(),
        json!({"owner_name": "[REDACTED]"})
    );
    assert_eq!(
        format!("{event:?}"),
        "AccountCreatedV1 { owner_name: \"[REDACTED]\" }"
    );
    assert_eq!(
        to_value(&event).unwrap(),
        json!({"owner_name": "private-owner-canary"})
    );
    let encoded = serde_json::to_vec(&event).unwrap();
    assert_eq!(
        decode::<AccountCreated>(
            &AccountCreated::STABLE_NAME,
            AccountCreated::VERSION,
            &encoded
        )
        .unwrap(),
        event
    );
}

#[test]
fn redaction_does_not_relax_the_storage_decoder() {
    for (version, bytes) in [
        (99, OLD),
        (
            1,
            br#"{"owner_name":"owner","balance_minor":5,"legacy_note":"note","extra":true}"#
                .as_slice(),
        ),
        (
            1,
            br#"{"owner_name":"owner","balance_minor":"wrong-type","legacy_note":"note"}"#
                .as_slice(),
        ),
    ] {
        assert!(decode::<AccountData>(
            &AccountData::STABLE_NAME,
            PayloadVersion::try_from_raw(version).unwrap(),
            bytes
        )
        .is_err());
    }
}

#[test]
fn template_derives_keep_historical_templates_and_structural_dual_output() {
    let previous_message: AccountMessageV1 = serde_json::from_value(json!({
        "owner_name": "private-owner-canary"
    }))
    .unwrap();
    let previous_dual: AccountDiagnosticV1 = serde_json::from_value(json!({
        "owner_name": "private-owner-canary"
    }))
    .unwrap();
    assert_eq!(format!("{previous_message:?}"), "Account [REDACTED]");
    assert_eq!(previous_message.to_redacted().text(), "Account [REDACTED]");
    assert_eq!(format!("{previous_dual:?}"), "Account [REDACTED]");
    assert_eq!(previous_dual.to_redacted().text(), "Account [REDACTED]");
    assert_eq!(
        previous_dual.to_redacted().json(),
        json!({"owner_name": "[REDACTED]"})
    );
    let message = AccountMessage {
        owner_name: "private-owner-canary".into(),
        kind: "account".into(),
    };
    let dual = AccountDiagnostic {
        owner_name: "private-owner-canary".into(),
        token: "private-token-canary".into(),
    };
    assert_eq!(format!("{message:?}"), "Account [REDACTED]");
    assert_eq!(message.to_redacted().text(), "Account [REDACTED]");
    assert_eq!(format!("{dual:?}"), "Account [REDACTED]");
    assert_eq!(dual.to_redacted().text(), "Account [REDACTED]");
    assert_eq!(
        dual.to_redacted().json(),
        json!({"owner_name":"[REDACTED]", "token":"[REDACTED]"})
    );
    assert_eq!(
        to_value(&message).unwrap(),
        json!({"owner_name":"private-owner-canary", "kind":"account"})
    );
    assert_eq!(
        to_value(&dual).unwrap(),
        json!({"owner_name":"private-owner-canary", "token":"private-token-canary"})
    );
}

#[test]
fn public_derives_keep_native_debug_and_explicitly_public_output() {
    let record = PublicRecord {
        kind: "account".into(),
    };
    let message = PublicMessage {
        kind: "account".into(),
    };
    assert_eq!(record.to_redacted().json(), json!({"kind":"account"}));
    assert_eq!(
        format!("{record:?}"),
        "PublicRecordV1 { kind: \"account\" }"
    );
    assert_eq!(format!("{message}"), "account");
    assert_eq!(message.to_redacted().text(), "account");
    assert_eq!(
        format!("{message:?}"),
        "PublicMessageV1 { kind: \"account\" }"
    );
    assert_eq!(to_value(&message).unwrap(), json!({"kind":"account"}));
    assert_eq!(record, record.clone());
}

#[test]
fn nested_supporting_records_and_enums_keep_their_own_policies() {
    let contacts = AccountContacts {
        contacts: vec![Contact {
            email: "private-contact-canary".into(),
        }],
        preferred: ContactChannel::Email {
            address: "private-preferred-canary".into(),
        },
    };
    assert_eq!(
        contacts.to_redacted().json(),
        json!({
            "contacts":[{"email":"[REDACTED]"}],
            "preferred":{"Email":{"address":"[REDACTED]"}}
        })
    );
    let raw = json!({"contacts":[{"email":"private-contact-canary"}], "preferred":{"Email":{"address":"private-preferred-canary"}}});
    assert_eq!(to_value(&contacts).unwrap(), raw);
    let bytes = rmp_serde::to_vec_named(&contacts).unwrap();
    let restored: AccountContacts = rmp_serde::from_slice(&bytes).unwrap();
    assert_eq!(restored, contacts);
    let debug = format!("{contacts:?}");
    assert!(!debug.contains("private-contact-canary"));
    assert!(!debug.contains("private-preferred-canary"));
    let old = br#"{"contacts":[{"email":"private-contact-canary"}]}"#;
    let current =
        decode::<AccountContacts>(&AccountContacts::STABLE_NAME, PayloadVersion::INITIAL, old)
            .unwrap();
    assert_eq!(current.contacts[0].email, "private-contact-canary");
    assert_eq!(
        current.to_redacted().json()["contacts"],
        json!([{"email":"[REDACTED]"}])
    );
    for invalid in [
        json!({"contacts":[{"email":"private", "extra":true}], "preferred":{"Email":{"address":"private"}}}),
        json!({"contacts":[], "preferred":{"Email":{"address":"private", "extra":true}}}),
    ] {
        assert!(serde_json::from_value::<AccountContacts>(invalid).is_err());
    }
}
