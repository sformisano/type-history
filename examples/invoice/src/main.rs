use invoice_history::Invoice;
use serde_json::{from_slice, to_vec};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let bytes = br#"{"billing.invoice.issued":{"1":{"legacy":"INV-42","count":42}}}"#;
    let historical = Invoice::from_versioned(from_slice(bytes)?)?;
    println!("Read an older invoice as the current type: {historical:?}");

    let current = Invoice {
        count: "84".to_owned(),
        label: "INV-84".to_owned(),
        revision: 8,
    };
    let bytes = to_vec(&current.into_versioned())?;
    // The application stores and retrieves these bytes using its chosen storage.
    let restored = Invoice::from_versioned(from_slice(&bytes)?)?;
    println!("Read back the current invoice: {restored:?}");
    Ok(())
}
