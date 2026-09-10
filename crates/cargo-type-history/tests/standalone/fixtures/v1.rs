use history_api::versioned;

#[versioned(stable_id = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
