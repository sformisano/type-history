use history_api::{Schema, versioned};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum OldStatus {
    Pending,
    Paid(u32),
}

#[versioned(stable_name = "billing.enum.payment")]
pub struct Payment {
    pub status: OldStatus,
    pub fallback: OldStatus,
    pub currency: String,
}
