#[typehistory::history(id = "shop.receipt.created")]
pub struct Receipt {
    #[not_sensitive]
    pub amount_cents: u64,
}
