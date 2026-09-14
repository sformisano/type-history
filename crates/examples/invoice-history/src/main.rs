use invoice_history::{payment::Payment, Invoice};
use serde_json::{from_slice, to_vec};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let bytes = br#"{"stable_name":"billing.invoice.issued","version":1,"payload":{"legacy":"INV-42","count":42}}"#;
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

    let stored = from_slice(include_bytes!("../tests/fixtures/payment-v1.json"))?;
    let payment = Payment::from_versioned(stored)?;
    println!("Read an older payment with its original amount and currency: {payment:?}");
    Ok(())
}
