use shop_types::ReceiptCreated;

fn main() {
    let receipt = ReceiptCreated {
        amount_cents: 3000,
        currency: std::env::args().nth(1).expect("currency argument"),
    };
    println!(
        "{}",
        serde_json::to_string(&receipt.into_versioned()).unwrap()
    );
}
