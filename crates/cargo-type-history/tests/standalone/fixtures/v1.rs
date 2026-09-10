use history_api::versioned;

#[versioned(stable_name = "billing.invoice.issued")]
pub struct Invoice {
    pub legacy: String,
    pub count: u32,
}
