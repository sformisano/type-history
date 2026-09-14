use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::versioned;

pub mod payment;

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
    #[history(removed_in = v2)]
    pub legacy: String,
    #[history(updated_in = v3, previous_type = u64, backfill_fn = stringify)]
    #[history(updated_in = v2, previous_type = u32, backfill_fn = widen)]
    pub count: String,
    #[history(added_in = v2, backfill_fn = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(previous: &InvoiceV1) -> Result<u64, ConvertError> {
    Ok(u64::from(previous.count))
}

fn label(previous: &InvoiceV1) -> Result<String, ConvertError> {
    Ok(previous.legacy.clone())
}

fn stringify(previous: &InvoiceV2) -> Result<String, ConvertError> {
    let value = previous.count;
    if value == 13 {
        Err(ConvertError(value))
    } else {
        Ok(value.to_string())
    }
}
