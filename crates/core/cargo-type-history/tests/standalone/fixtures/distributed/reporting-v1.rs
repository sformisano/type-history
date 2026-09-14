use shop_types::ReceiptCreated;
use std::{error::Error, fs};

fn read_receipt() -> Result<(), Box<dyn Error>> {
    let path = std::env::args().nth(1).expect("queued event path");
    let bytes = fs::read(path)?;
    let receipt = ReceiptCreated::from_versioned(serde_json::from_slice(&bytes)?)?;
    // V1's business rule: every amount means USD.
    println!(
        "{}",
        serde_json::json!({"amount_cents": receipt.amount_cents, "currency": "USD"})
    );
    Ok(())
}

fn main() {
    if let Err(error) = read_receipt() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
