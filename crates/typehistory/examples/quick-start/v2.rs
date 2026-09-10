#[typehistory::history(id = "shop.receipt.created")]
pub struct Receipt {
    #[not_sensitive]
    pub amount_cents: u64,

    #[not_sensitive]
    #[history(added_at = v2, backfill_value = "USD".to_owned())]
    pub currency: String,
}
