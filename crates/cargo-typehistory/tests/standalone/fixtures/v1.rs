use history_api::typehistory;

#[typehistory(stable_id = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
