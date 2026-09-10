use history_api::versioned;

#[versioned(stable_id = "receipt_created")]
pub struct ReceiptCreated {
    pub amount: u32,
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

#[versioned(stable_id = "shop.receipt")]
pub struct Receipt {
    pub amount: u32,
    #[history(added_at = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

#[cfg(test)]
mod tests {
    use super::{Receipt, ReceiptCreated};
    use history_api::Versioned;
    use serde_json::{from_str, json, to_value};

    #[test]
    fn simple_id_reads_older_data_and_writes_the_current_version() {
        let old = r#"{"receipt_created":{"1":{"amount":42}}}"#;
        let stored: Versioned<ReceiptCreated> = from_str(old).unwrap();
        let current = ReceiptCreated::from_versioned(stored).unwrap();
        assert_eq!(current.amount, 42);
        assert_eq!(current.revision, 7);
        assert_eq!(
            to_value(current.into_versioned()).unwrap(),
            json!({"receipt_created":{"2":{"amount":42,"revision":7}}})
        );
    }

    #[test]
    fn optional_namespace_reads_older_data_and_writes_the_current_version() {
        let old = r#"{"shop.receipt":{"1":{"amount":42}}}"#;
        let stored: Versioned<Receipt> = from_str(old).unwrap();
        let current = Receipt::from_versioned(stored).unwrap();
        assert_eq!(current.amount, 42);
        assert_eq!(current.revision, 7);
        assert_eq!(
            to_value(current.into_versioned()).unwrap(),
            json!({"shop.receipt":{"2":{"amount":42,"revision":7}}})
        );
    }
}
