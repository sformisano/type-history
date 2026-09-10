#[typehistory::history(id = "shop.receipt.created")]
pub struct Receipt {
    pub amount_cents: u64,

    #[history(added_at = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}
