#[typehistory::history(id = "billing.invoice.issued")]
pub struct Invoice {
    #[sensitive(redactable::Secret)]
    pub legacy: String,
    #[not_sensitive]
    pub count: u32,
}
