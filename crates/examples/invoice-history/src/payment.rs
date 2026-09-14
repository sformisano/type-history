//! A payment whose status evolves through the containing record's history.

use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fmt::{Display, Formatter, Result as FmtResult},
};
use type_history::{versioned, Schema};

/// Original status definition, retained for stored V1 payments.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub enum PaymentStatusV1 {
    Pending,
    Paid(u64),
}

/// Supported currencies for current paid amounts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum Currency {
    Usd,
    Eur,
}

/// Current status, with an explicit currency beside every paid amount.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub enum PaymentStatus {
    Pending,
    Paid {
        amount_minor: u64,
        currency: Currency,
    },
}

#[versioned(stable_name = "billing.payment.recorded")]
pub struct Payment {
    pub reference: String,
    #[history(removed_in = v2)]
    pub currency_code: String,
    #[history(updated_in = v2, previous_type = PaymentStatusV1, backfill_fn = update_status)]
    pub status: PaymentStatus,
}

/// An old payment used a currency this example cannot interpret.
#[derive(Debug)]
pub struct PaymentCurrencyError(pub String);

impl Display for PaymentCurrencyError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        write!(formatter, "unsupported stored currency {}", self.0)
    }
}

impl Error for PaymentCurrencyError {}

fn update_status(previous: &PaymentV1) -> Result<PaymentStatus, PaymentCurrencyError> {
    match previous.status {
        PaymentStatusV1::Pending => Ok(PaymentStatus::Pending),
        PaymentStatusV1::Paid(amount_minor) => {
            let currency = match previous.currency_code.as_str() {
                "USD" => Currency::Usd,
                "EUR" => Currency::Eur,
                _ => return Err(PaymentCurrencyError(previous.currency_code.clone())),
            };
            Ok(PaymentStatus::Paid {
                amount_minor,
                currency,
            })
        }
    }
}
