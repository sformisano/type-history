use history_api::history;

#[history(id = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
