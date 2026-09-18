//! Named-field admission comes from the wire contract, never value nullability.

use super::super::{
    ConstantFields, ConstantShape, ConstantVariant, ConstantVariants, FieldPresence,
};
use schemars::Schema;
use serde_json::{Map, Value};

/// Correct a generated record or enum schema's named-field presence declarations.
///
/// Externally tagged enum wrappers keep their required tag. Only named record
/// payloads receive field presence declarations; nested producers correct their
/// own schemas before this function is called.
#[doc(hidden)]
pub fn apply_named_presence(schema: &mut Schema, shape: &ConstantShape) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    match shape {
        ConstantShape::Record(fields) => record(object, fields),
        ConstantShape::Enum(variants) => enumeration(object, variants),
        _ => {}
    }
}

fn record(object: &mut Map<String, Value>, mut fields: &ConstantFields) {
    let mut required = Vec::new();
    while let ConstantFields::Field(name, presence, _, tail) = fields {
        if matches!(presence, FieldPresence::Required) {
            required.push(name.name());
        }
        fields = tail;
    }
    required.sort();
    object.insert(
        "required".to_owned(),
        Value::Array(required.into_iter().map(Value::String).collect()),
    );
}

fn enumeration(object: &mut Map<String, Value>, variants: &ConstantVariants) {
    // Schemars uses oneOf for externally tagged variants. Accept anyOf as well
    // because merging equivalent alternatives does not change the tag wrapper.
    for key in ["oneOf", "anyOf"] {
        if let Some(Value::Array(branches)) = object.get_mut(key) {
            for branch in branches {
                if let Some(branch) = branch.as_object_mut() {
                    enumeration(branch, variants);
                }
            }
        }
    }
    let Some(Value::Object(properties)) = object.get_mut("properties") else {
        return;
    };
    let mut next = variants;
    while let ConstantVariants::Variant(name, variant, tail) = next {
        if let ConstantVariant::Record(fields)
        | ConstantVariant::Newtype(ConstantShape::Record(fields)) = variant
        {
            if let Some(Value::Object(payload)) = properties.get_mut(&name.name()) {
                record(payload, fields);
            }
        }
        next = tail;
    }
}

#[cfg(test)]
mod tests {
    use super::apply_named_presence;
    use crate::resolved::{
        ConstantFields, ConstantShape, ConstantVariant, ConstantVariants, End, Field, Item,
        NameByte, NameEnd, Newtype, Optional, Record, WireFields, WireName, WireNode, U32,
    };
    use serde_json::json;

    type A = NameByte<b'a', NameEnd>;
    type Z = NameByte<b'z', NameEnd>;
    type Fields = Item<Field<Z, Newtype<Optional<U32>>>, Item<Field<A, Optional<U32>>, End>>;

    #[test]
    fn required_nullable_fields_are_not_made_omittable() {
        let mut schema = schemars::json_schema!({
            "type": "object", "required": ["a"],
            "properties": {"z": {"type": ["integer", "null"]}, "a": {"type": ["integer", "null"]}}
        });
        apply_named_presence(&mut schema, &Record::<Fields>::SHAPE);
        assert_eq!(schema.as_value()["required"], json!(["z"]));
        apply_named_presence(&mut schema, &ConstantShape::Record(ConstantFields::End));
        assert_eq!(schema.as_value()["required"], json!([]));
    }

    #[test]
    fn enum_record_payload_correction_keeps_tag_required() {
        let variants = ConstantVariants::Variant(
            A::NAME,
            ConstantVariant::Record(Fields::FIELDS),
            &ConstantVariants::Variant(
                Z::NAME,
                ConstantVariant::Newtype(&Optional::<U32>::SHAPE),
                &ConstantVariants::End,
            ),
        );
        let mut schema = schemars::json_schema!({"oneOf": [
            {"type": "object", "required": ["a"], "properties": {"a": {
                "type": "object", "required": [], "properties": {"a": {}, "z": {}}
            }}},
            {"type": "object", "required": ["z"], "properties": {"z": {"type": ["integer", "null"]}}}
        ]});
        apply_named_presence(&mut schema, &ConstantShape::Enum(variants));
        let value = schema.as_value();
        assert_eq!(value["oneOf"][0]["required"], json!(["a"]));
        assert_eq!(
            value["oneOf"][0]["properties"]["a"]["required"],
            json!(["z"])
        );
        assert_eq!(value["oneOf"][1]["required"], json!(["z"]));
        assert!(value["oneOf"][1]["properties"]["z"]
            .get("required")
            .is_none());
    }
}
