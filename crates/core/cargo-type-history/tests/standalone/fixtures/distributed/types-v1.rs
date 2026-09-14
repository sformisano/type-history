use history_api::versioned;

#[versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,
}
