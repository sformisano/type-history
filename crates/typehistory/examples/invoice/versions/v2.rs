use redactable::{BypassTextRedaction, RedactedValue, ToRedacted};

#[derive(Debug)]
pub struct ConvertError;

impl ToRedacted for ConvertError {
    fn to_redacted(&self) -> RedactedValue {
        BypassTextRedaction("safe conversion failure".to_owned()).to_redacted()
    }
}

#[typehistory::history(id = "billing.invoice.issued")]
pub struct Invoice {
    #[sensitive(redactable::Secret)]
    #[history(removed_at = v2)]
    pub legacy: String,
    #[not_sensitive]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: u64,
    #[sensitive(redactable::Secret)]
    #[history(added_at = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[not_sensitive]
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    Ok(u64::from(value))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}
