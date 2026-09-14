use history_api::versioned;
use std::error::Error;
use std::fmt::{Display, Formatter, Result as FmtResult};

#[derive(Debug)]
pub struct ConvertError;
impl Display for ConvertError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("conversion failed")
    }
}
impl Error for ConvertError {}

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    #[history(removed_in = v2)]
    pub legacy: String,
    #[history(updated_in = v2, previous_type = u32, backfill_fn = widen)]
    pub count: u64,
    #[history(added_in = v2, backfill_fn = label)]
    pub label: String,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

fn widen(previous: &InvoiceV1) -> Result<u64, ConvertError> { Ok(u64::from(previous.count)) }
fn label(previous: &InvoiceV1) -> Result<String, ConvertError> { Ok(previous.legacy.clone()) }
