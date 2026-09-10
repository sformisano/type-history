pub mod addresses {
    #[derive(
        Clone,
        Debug,
        PartialEq,
        serde::Serialize,
        serde::Deserialize,
        redactable::NotSensitive,
        typehistory::Schema,
    )]
    #[serde(deny_unknown_fields)]
    pub struct Address {
        pub number: u32,
    }

    #[typehistory::history(id = "billing.address.record")]
    pub struct AddressRecord {
        pub address: Address,
    }

    #[test]
    fn nested_values_keep_their_closed_codec() {
        use typehistory::{HasHistory, PayloadVersion};

        assert!(AddressRecord::history()
            .decode(PayloadVersion::INITIAL, br#"{"address":{"number":7}}"#)
            .is_ok());
        assert!(AddressRecord::history()
            .decode(
                PayloadVersion::INITIAL,
                br#"{"address":{"number":7,"unknown":1}}"#,
            )
            .is_err());
    }
}
