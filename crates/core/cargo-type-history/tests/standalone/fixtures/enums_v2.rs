use history_api::{Schema, versioned};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
pub enum OldStatus {
    Pending,
    Paid(u32),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Schema)]
#[serde(deny_unknown_fields)]
pub enum Status {
    Pending,
    Paid { amount: u64, currency: String },
}

#[versioned(stable_name = "billing.enum.payment")]
pub struct Payment {
    #[history(updated_in = v2, previous_type = OldStatus, backfill_fn = upgrade)]
    pub status: Status,
    #[history(updated_in = v2, previous_type = OldStatus, backfill_value = Status::Pending)]
    pub fallback: Status,
    pub currency: String,
}

fn upgrade(old: &PaymentV1) -> Result<Status, Infallible> {
    Ok(match old.status {
        OldStatus::Pending => Status::Pending,
        OldStatus::Paid(amount) => Status::Paid {
            amount: u64::from(amount),
            currency: old.currency.clone(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{Payment, Status};
    use history_api::Versioned;

    const OLD_JSON: &[u8] = br#"{"stable_name":"billing.enum.payment","version":1,"payload":{"status":{"Paid":4200},"fallback":{"Paid":7},"currency":"USD"}}"#;
    const OLD_MESSAGEPACK: &[u8] = b"\x83\xabstable_name\xb4billing.enum.payment\xa7version\x01\xa7payload\x83\xa6status\x81\xa4Paid\xcd\x10\x68\xa8fallback\x81\xa4Paid\x07\xa8currency\xa3USD";

    #[test]
    fn fixed_old_bytes_keep_amount_currency_and_status() {
        let json: Versioned<Payment> = serde_json::from_slice(OLD_JSON).unwrap();
        let binary: Versioned<Payment> = rmp_serde::from_slice(OLD_MESSAGEPACK).unwrap();
        for stored in [json, binary] {
            let payment = Payment::from_versioned(stored).unwrap();
            assert_eq!(
                payment.status,
                Status::Paid {
                    amount: 4200,
                    currency: "USD".into()
                }
            );
            assert_eq!(payment.fallback, Status::Pending);
            assert_eq!(payment.currency, "USD");
        }
    }
}
