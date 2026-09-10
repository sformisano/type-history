use invoice_history::Invoice;
use std::error::Error;
use typehistory::{decode, HasHistory, PayloadVersion};

fn main() -> Result<(), Box<dyn Error>> {
    let historical = decode::<Invoice>(
        &Invoice::TYPE,
        PayloadVersion::INITIAL,
        br#"{"legacy":"INV-42","count":42}"#,
    )?;
    println!("Read V1 as V{}: {historical:?}", Invoice::VERSION.get());

    let current = Invoice {
        count: "84".to_owned(),
        label: "INV-84".to_owned(),
        revision: 8,
    };
    let payload = serde_json::to_string(&current)?;
    println!(
        "Store {} at V{}: {payload}",
        Invoice::TYPE,
        Invoice::VERSION.get()
    );
    Ok(())
}
