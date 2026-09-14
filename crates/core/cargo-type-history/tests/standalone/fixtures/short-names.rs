use history_api::versioned;

#[versioned(stable_name = "receipt_created")]
pub struct ReceiptCreated {
    pub amount: u32,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

#[versioned(stable_name = "shop.receipt")]
pub struct Receipt {
    pub amount: u32,
    #[history(added_in = v2, backfill_value = 7_u32)]
    pub revision: u32,
}

#[cfg(test)]
mod tests {
    use super::{Receipt, ReceiptCreated};
    use history_api::Versioned;
    use serde_json::{from_str, json, to_value};

    #[test]
    fn simple_name_reads_older_data_and_writes_the_current_version() {
        let old = r#"{"stable_name":"receipt_created","version":1,"payload":{"amount":42}}"#;
        let stored: Versioned<ReceiptCreated> = from_str(old).unwrap();
        let current = ReceiptCreated::from_versioned(stored).unwrap();
        assert_eq!(current.amount, 42);
        assert_eq!(current.revision, 7);
        assert_eq!(
            to_value(current.into_versioned()).unwrap(),
            json!({"stable_name":"receipt_created","version":2,"payload":{"amount":42,"revision":7}})
        );
    }

    #[test]
    fn optional_namespace_reads_older_data_and_writes_the_current_version() {
        let old = r#"{"stable_name":"shop.receipt","version":1,"payload":{"amount":42}}"#;
        let stored: Versioned<Receipt> = from_str(old).unwrap();
        let current = Receipt::from_versioned(stored).unwrap();
        assert_eq!(current.amount, 42);
        assert_eq!(current.revision, 7);
        assert_eq!(
            to_value(current.into_versioned()).unwrap(),
            json!({"stable_name":"shop.receipt","version":2,"payload":{"amount":42,"revision":7}})
        );
    }
}
