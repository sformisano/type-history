use history_api::versioned;

#[versioned(stable_name = "shop.receipt.created")]
pub struct ReceiptCreated {
    pub amount_cents: u64,
    #[history(added_in = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}
