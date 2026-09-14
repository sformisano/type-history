use history_api::{versioned, Schema};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Schema, Serialize, Deserialize)]
pub struct Details {
    pub amount: u32,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Schema, Serialize, Deserialize)]
pub enum Payment {
    Cash,
    Card(Details),
    Split(u32, bool),
    Transfer { reference: u32, confirmed: bool },
}

#[versioned(stable_name = "billing.diagnostic")]
pub struct Invoice {
    pub details: Option<Vec<Details>>,
    pub payment: Payment,
    pub counts: [u32; 2],
}
