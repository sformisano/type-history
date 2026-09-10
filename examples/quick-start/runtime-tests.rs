#[cfg(test)]
mod receipt_tests {
    use super::Receipt;
    use typehistory::{HasHistory, PayloadVersion, decode};

    #[test]
    fn read_an_old_receipt() {
        let receipt = decode::<Receipt>(
            &Receipt::TYPE,
            PayloadVersion::INITIAL,
            br#"{"amount_cents":1200}"#,
        )
        .unwrap();

        assert_eq!(receipt.amount_cents, 1200);
        assert_eq!(receipt.currency, "USD");
        assert_eq!(Receipt::VERSION.get(), 2);
    }

    #[test]
    fn write_a_current_receipt() {
        let receipt = Receipt {
            amount_cents: 3000,
            currency: "EUR".to_owned(),
        };
        let identity = Receipt::TYPE;
        let version = Receipt::VERSION;
        let bytes = serde_json::to_vec(&receipt).unwrap();

        // Persist identity, version, and bytes together in your storage format.
        let restored = decode::<Receipt>(&identity, version, &bytes).unwrap();
        assert_eq!(restored, receipt);
        assert_eq!(restored.currency, "EUR");
    }
}
