use redactable::{Secret, Sensitive};
use type_history::versioned;

#[versioned(stable_name = "account.data", derive_debug = false)]
#[derive(Sensitive, Eq)]
pub struct AccountData {
    #[sensitive(Secret)]
    pub owner_name: String,
    #[history(updated_in = v2, previous_type = u64, backfill_fn = widen_balance)]
    #[sensitive(Secret)]
    pub balance_minor: u128,
    #[history(removed_in = v2)]
    #[sensitive(Secret)]
    pub legacy_note: String,
    #[history(added_in = v2, backfill_value = String::from("account"))]
    #[not_sensitive]
    pub record_kind: String,
}

fn widen_balance(previous: &AccountDataV1) -> Result<u128, std::convert::Infallible> {
    Ok(u128::from(previous.balance_minor))
}

#[versioned(stable_name = "account.created", derive_debug = false)]
#[derive(redactable::Sensitive)]
pub struct AccountCreated {
    #[sensitive(Secret)]
    pub owner_name: String,
}

#[versioned(stable_name = "redaction.message", derive_debug = false)]
#[derive(redactable::SensitiveDisplay)]
/// Account {owner_name}
pub struct AccountMessage {
    #[sensitive(Secret)]
    pub owner_name: String,
    #[history(added_in = v2, backfill_value = String::from("account"))]
    #[not_sensitive]
    pub kind: String,
}

#[versioned(stable_name = "redaction.dual", derive_debug = false)]
#[derive(redactable::SensitiveDual)]
#[error("Account {owner_name}")]
pub struct AccountDiagnostic {
    #[sensitive(Secret)]
    pub owner_name: String,
    #[history(added_in = v2, backfill_value = String::new())]
    #[sensitive(Secret)]
    pub token: String,
}

#[versioned(stable_name = "redaction.public")]
#[derive(redactable::NotSensitive, Eq)]
pub struct PublicRecord {
    pub kind: String,
}

#[versioned(stable_name = "redaction.public_display")]
#[derive(redactable::NotSensitiveDisplay)]
pub struct PublicMessage {
    pub kind: String,
}

impl std::fmt::Display for PublicMessageV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.kind)
    }
}

#[derive(
    Clone, PartialEq, serde::Serialize, serde::Deserialize, type_history::Schema, Sensitive,
)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    #[sensitive(Secret)]
    pub email: String,
}

#[derive(
    Clone, PartialEq, serde::Serialize, serde::Deserialize, type_history::Schema, Sensitive,
)]
#[serde(deny_unknown_fields)]
pub enum ContactChannel {
    Email {
        #[sensitive(Secret)]
        address: String,
    },
}

#[versioned(stable_name = "redaction.contacts", derive_debug = false)]
#[derive(Sensitive)]
pub struct AccountContacts {
    pub contacts: Vec<Contact>,
    #[history(added_in = v2, backfill_value = ContactChannel::Email { address: String::new() })]
    pub preferred: ContactChannel,
}
