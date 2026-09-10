use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::versioned;

#[derive(Debug)]
pub struct ConvertError(pub u64);

impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "count {} cannot be converted", self.0)
    }
}

impl Error for ConvertError {}

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_at = v2)]
    pub legacy: String,
    #[history(updated_at = v3, previous_type = u64, cast_from_previous_value = stringify)]
    #[history(updated_at = v2, previous_type = u32, cast_from_previous_value = widen)]
    pub count: String,
    #[history(added_in = v2, cast_from_previous_payload = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(value: u32) -> Result<u64, ConvertError> {
    Ok(u64::from(value))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}

fn stringify(value: u64) -> Result<String, ConvertError> {
    if value == 13 {
        Err(ConvertError(value))
    } else {
        Ok(value.to_string())
    }
}
