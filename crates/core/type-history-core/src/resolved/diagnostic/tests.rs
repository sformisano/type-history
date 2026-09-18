use serde_json::Value;

use super::{encode, encoded_len};
use crate::resolved::{
    ConstantFields, ConstantItems, ConstantMembership, ConstantName, ConstantShape,
    ConstantVariant, ConstantVariants, FieldPresence, StorageProfile,
};

const NAME: &str = "a\0\u{1}\u{2}\u{3}\u{4}\u{5}\u{6}\u{7}\u{8}\t\n\u{b}\u{c}\r\u{e}\u{f}\u{10}\u{11}\u{12}\u{13}\u{14}\u{15}\u{16}\u{17}\u{18}\u{19}\u{1a}\u{1b}\u{1c}\u{1d}\u{1e}\u{1f}\"\\é🦀";
const FIELD: ConstantName = ConstantName::Byte(
    b'"',
    &ConstantName::Byte(
        b'\\',
        &ConstantName::Byte(0xc3, &ConstantName::Byte(0xa9, &ConstantName::End)),
    ),
);
const FIELDS: ConstantFields = ConstantFields::Field(
    FIELD,
    FieldPresence::Optional,
    &ConstantShape::Option(&ConstantShape::U32),
    &ConstantFields::Field(
        ConstantName::End,
        FieldPresence::Required,
        &ConstantShape::String,
        &ConstantFields::End,
    ),
);
const ITEMS: ConstantItems = ConstantItems::Item(
    &ConstantShape::Bool,
    &ConstantItems::Item(&ConstantShape::Record(FIELDS), &ConstantItems::End),
);

fn assert_message(bytes: &[u8], expected: ConstantShape, actual: ConstantShape) {
    let message = str::from_utf8(bytes).expect("safe UTF-8");
    let (prefix, json) = message
        .split_once("TYPE_HISTORY_SCHEMA_DIAGNOSTIC_V1:")
        .unwrap();
    assert!(prefix.contains("restore its history and add the next version"));
    let value: Value = serde_json::from_str(json).unwrap();
    assert_eq!(value["stable_name"], NAME);
    assert_eq!(value["version"], u32::MAX);
    assert_eq!(
        value["expected"],
        serde_json::to_value(expected.schema()).unwrap()
    );
    assert_eq!(
        value["actual"],
        serde_json::to_value(actual.schema()).unwrap()
    );
}

macro_rules! check {
    ($shape:expr) => {{
        const EXPECTED: ConstantShape = $shape;
        const ACTUAL: ConstantShape = if matches!(EXPECTED, ConstantShape::Bool) {
            ConstantShape::String
        } else {
            ConstantShape::Bool
        };
        const LENGTH: usize = encoded_len(NAME, u32::MAX, &EXPECTED, &ACTUAL);
        const BYTES: [u8; LENGTH] = encode(NAME, u32::MAX, &EXPECTED, &ACTUAL);
        assert_message(&BYTES, EXPECTED, ACTUAL);
        const REVERSE: [u8; LENGTH] = encode(NAME, u32::MAX, &ACTUAL, &EXPECTED);
        assert_message(&REVERSE, ACTUAL, EXPECTED);
    }};
}

#[test]
fn const_encoder_matches_runtime_descriptors_for_every_node_and_container() {
    check!(ConstantShape::Bool);
    check!(ConstantShape::I8);
    check!(ConstantShape::I16);
    check!(ConstantShape::I32);
    check!(ConstantShape::I64);
    check!(ConstantShape::I128);
    check!(ConstantShape::U8);
    check!(ConstantShape::U16);
    check!(ConstantShape::U32);
    check!(ConstantShape::U64);
    check!(ConstantShape::U128);
    check!(ConstantShape::String);
    check!(ConstantShape::Bytes);
    check!(ConstantShape::Option(&ConstantShape::U32));
    check!(ConstantShape::Sequence(&ConstantShape::Record(FIELDS)));
    check!(ConstantShape::Array(&ConstantShape::U32, usize::MAX));
    check!(ConstantShape::Array(&ConstantShape::U32, 0));
    check!(ConstantShape::Record(ConstantFields::End));
    check!(ConstantShape::Record(FIELDS));
    check!(ConstantShape::Enum(ConstantVariants::End));
}

#[test]
fn const_encoder_matches_every_variant_and_legacy_record_newtype() {
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Unit,
        &ConstantVariants::End
    )));
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Newtype(&ConstantShape::Bytes),
        &ConstantVariants::End
    )));
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Tuple(ConstantItems::End),
        &ConstantVariants::End
    )));
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Tuple(ITEMS),
        &ConstantVariants::End
    )));
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Record(FIELDS),
        &ConstantVariants::Variant(
            ConstantName::End,
            ConstantVariant::Unit,
            &ConstantVariants::End
        )
    )));
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Newtype(&ConstantShape::Record(FIELDS)),
        &ConstantVariants::End
    )));
}

#[test]
fn equal_shapes_use_no_buffer_or_serialization() {
    // A malformed UTF-8 name would fail message decoding if traversed.
    const SHAPE: ConstantShape = ConstantShape::Record(ConstantFields::Field(
        ConstantName::Byte(255, &ConstantName::End),
        FieldPresence::Required,
        &ConstantShape::U8,
        &ConstantFields::End,
    ));
    const LENGTH: usize = encoded_len("same", 1, &SHAPE, &SHAPE);
    const BYTES: [u8; LENGTH] = encode("same", 1, &SHAPE, &SHAPE);
    assert_eq!(LENGTH, 0);
    assert!(BYTES.is_empty());
}

#[test]
#[should_panic(expected = "incorrect frozen schema diagnostic buffer size")]
fn oversized_buffers_are_rejected() {
    encode::<1>("same", 1, &ConstantShape::U8, &ConstantShape::U8);
}

#[test]
#[should_panic]
fn undersized_buffers_are_rejected() {
    encode::<1>("changed", 1, &ConstantShape::U8, &ConstantShape::U32);
}

#[test]
fn extended_diagnostics_preserve_profile_membership_tuple_and_presence() {
    const MEMBER: ConstantMembership = ConstantMembership {
        id: "example:tuple:v1",
        parameters: &[ConstantMembership::custom("example:exact:v1")],
    };
    check!(ConstantShape::Map(&ConstantShape::U32));
    check!(ConstantShape::Set(&ConstantShape::U8, MEMBER));
    check!(ConstantShape::Tuple(ITEMS));
    check!(ConstantShape::Profile(StorageProfile::Finite32));
    check!(ConstantShape::Profile(StorageProfile::Finite64));
    check!(ConstantShape::Profile(StorageProfile::UuidText));
    check!(ConstantShape::Profile(StorageProfile::DecimalText));
    check!(ConstantShape::Profile(StorageProfile::Date));
    check!(ConstantShape::Profile(StorageProfile::LocalTime));
    check!(ConstantShape::Profile(StorageProfile::LocalDateTime));
    check!(ConstantShape::Profile(StorageProfile::UtcInstant));
    check!(ConstantShape::Profile(StorageProfile::OffsetDateTime));
    check!(ConstantShape::Enum(ConstantVariants::Variant(
        FIELD,
        ConstantVariant::Newtype(&ConstantShape::Tuple(ITEMS)),
        &ConstantVariants::End,
    )));
    const REQUIRED: ConstantShape = ConstantShape::Record(ConstantFields::Field(
        FIELD,
        FieldPresence::Required,
        &ConstantShape::Option(&ConstantShape::U32),
        &ConstantFields::End,
    ));
    const OPTIONAL: ConstantShape = ConstantShape::Record(ConstantFields::Field(
        FIELD,
        FieldPresence::Optional,
        &ConstantShape::Option(&ConstantShape::U32),
        &ConstantFields::End,
    ));
    const LENGTH: usize = encoded_len(NAME, u32::MAX, &REQUIRED, &OPTIONAL);
    const BYTES: [u8; LENGTH] = encode(NAME, u32::MAX, &REQUIRED, &OPTIONAL);
    assert_message(&BYTES, REQUIRED, OPTIONAL);
}
