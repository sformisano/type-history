use super::{SchemaField, SchemaShape, SchemaVariant, SchemaVariantShape};
use serde::de::{Error, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use std::fmt::{Formatter, Result as FmtResult};

const FIELDS: &[&str] = &["name", "kind", "schema", "items", "fields"];
const KINDS: &[&str] = &["unit", "newtype", "tuple", "record"];

impl<'de> Deserialize<'de> for SchemaVariant {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(SchemaVariantVisitor)
    }
}

struct SchemaVariantVisitor;

impl<'de> Visitor<'de> for SchemaVariantVisitor {
    type Value = SchemaVariant;

    fn expecting(&self, formatter: &mut Formatter<'_>) -> FmtResult {
        formatter.write_str("a named enum variant descriptor")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut name = None;
        let mut kind = None;
        let mut schema = None;
        let mut items = None;
        let mut fields = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "name" => {
                    if name.is_some() {
                        return Err(A::Error::duplicate_field("name"));
                    }
                    name = Some(map.next_value()?);
                }
                "kind" => {
                    if kind.is_some() {
                        return Err(A::Error::duplicate_field("kind"));
                    }
                    kind = Some(map.next_value()?);
                }
                "schema" => {
                    if schema.is_some() {
                        return Err(A::Error::duplicate_field("schema"));
                    }
                    schema = Some(map.next_value::<Box<SchemaShape>>()?);
                }
                "items" => {
                    if items.is_some() {
                        return Err(A::Error::duplicate_field("items"));
                    }
                    items = Some(map.next_value::<Vec<SchemaShape>>()?);
                }
                "fields" => {
                    if fields.is_some() {
                        return Err(A::Error::duplicate_field("fields"));
                    }
                    fields = Some(map.next_value::<Vec<SchemaField>>()?);
                }
                _ => return Err(A::Error::unknown_field(&key, FIELDS)),
            }
        }

        let name = name.ok_or_else(|| A::Error::missing_field("name"))?;
        let kind: String = kind.ok_or_else(|| A::Error::missing_field("kind"))?;
        let shape = match kind.as_str() {
            "unit" => {
                reject_present::<A::Error, _>(schema.as_ref(), "schema", "unit")?;
                reject_present::<A::Error, _>(items.as_ref(), "items", "unit")?;
                reject_present::<A::Error, _>(fields.as_ref(), "fields", "unit")?;
                SchemaVariantShape::Unit
            }
            "newtype" => {
                reject_present::<A::Error, _>(items.as_ref(), "items", "newtype")?;
                reject_present::<A::Error, _>(fields.as_ref(), "fields", "newtype")?;
                SchemaVariantShape::Newtype {
                    schema: schema.ok_or_else(|| A::Error::missing_field("schema"))?,
                }
            }
            "tuple" => {
                reject_present::<A::Error, _>(schema.as_ref(), "schema", "tuple")?;
                reject_present::<A::Error, _>(fields.as_ref(), "fields", "tuple")?;
                SchemaVariantShape::Tuple {
                    items: items.ok_or_else(|| A::Error::missing_field("items"))?,
                }
            }
            "record" => {
                reject_present::<A::Error, _>(schema.as_ref(), "schema", "record")?;
                reject_present::<A::Error, _>(items.as_ref(), "items", "record")?;
                SchemaVariantShape::Record {
                    fields: fields.ok_or_else(|| A::Error::missing_field("fields"))?,
                }
            }
            _ => return Err(A::Error::unknown_variant(&kind, KINDS)),
        };

        Ok(SchemaVariant { name, shape })
    }
}

fn reject_present<E, T>(value: Option<&T>, field: &'static str, kind: &str) -> Result<(), E>
where
    E: Error,
{
    if value.is_some() {
        return Err(E::custom(format_args!(
            "field `{field}` is not valid for variant kind `{kind}`"
        )));
    }
    Ok(())
}
