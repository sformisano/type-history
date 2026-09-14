use shop_types::ReceiptCreated;

fn main() {
    let receipt = ReceiptCreated { amount_cents: 1200 };
    println!(
        "{}",
        serde_json::to_string(&receipt.into_versioned()).unwrap()
    );
}
