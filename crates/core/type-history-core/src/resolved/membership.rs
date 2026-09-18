//! Explicit, recursively comparable declarations of set element equality.

mod builtins;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

/// Declares the equality rule used when this type is a set element.
///
/// Implementors must use a stable, nonempty ID and change that ID whenever
/// equality changes. This declaration is trusted: deriving a schema does not
/// certify `Eq`, `Hash`, or `Ord`, and collections are not scanned for duplicates.
pub trait SetMembership {
    /// Exact membership rule, including component rules in positional order.
    const MEMBERSHIP: ConstantMembership;
}

/// Allocation-free membership declaration used by frozen schema assertions.
#[derive(Debug, Clone, Copy)]
pub struct ConstantMembership {
    /// Stable nonempty rule identity; application IDs should be namespaced.
    pub id: &'static str,
    /// Ordered component membership declarations.
    pub parameters: &'static [ConstantMembership],
}

impl ConstantMembership {
    /// Declare a caller-owned leaf membership rule.
    ///
    /// # Panics
    /// Panics if `id` is empty.
    pub const fn custom(id: &'static str) -> Self {
        assert!(!id.is_empty(), "membership ID must not be empty");
        Self {
            id,
            parameters: &[],
        }
    }

    /// Compare the complete tree without allocating.
    pub const fn same(&self, other: &Self) -> bool {
        let a = self.id.as_bytes();
        let b = other.id.as_bytes();
        if a.len() != b.len() || self.parameters.len() != other.parameters.len() {
            return false;
        }
        let mut index = 0;
        while index < a.len() {
            if a[index] != b[index] {
                return false;
            }
            index += 1;
        }
        index = 0;
        while index < self.parameters.len() {
            if !self.parameters[index].same(&other.parameters[index]) {
                return false;
            }
            index += 1;
        }
        true
    }

    /// Build the equivalent runtime declaration.
    pub fn membership(&self) -> Membership {
        Membership {
            id: self.id.to_owned(),
            parameters: self.parameters.iter().map(Self::membership).collect(),
        }
    }

    /// Encode the shared JSON Schema membership extension.
    pub fn to_json(&self) -> Value {
        self.membership().to_json()
    }
}

/// Persisted membership declaration. IDs are explicit author contracts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Membership {
    /// Stable nonempty rule identity.
    #[serde(deserialize_with = "nonempty_id")]
    pub id: String,
    /// Ordered component rules; empty for a leaf.
    pub parameters: Vec<Membership>,
}

impl Membership {
    /// Encode the shared JSON Schema membership extension.
    pub fn to_json(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "parameters": self.parameters.iter().map(Self::to_json).collect::<Vec<_>>(),
        })
    }
}

fn nonempty_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let id = String::deserialize(deserializer)?;
    if id.is_empty() {
        return Err(D::Error::custom("membership ID must not be empty"));
    }
    Ok(id)
}

/// Reserved built-in rule identities. Composite nodes append their ordered
/// component declarations without changing the rule ID.
#[doc(hidden)]
pub mod rules {
    use super::ConstantMembership;
    macro_rules! rules {
        ($($name:ident => $rule:literal),+ $(,)?) => {
            $(pub const $name: ConstantMembership = ConstantMembership::custom(
                concat!("type-history:membership:", $rule, ":v1")
            );)+
        };
    }
    rules! {
        BOOL => "bool", I8 => "i8", I16 => "i16", I32 => "i32", I64 => "i64", I128 => "i128",
        U8 => "u8", U16 => "u16", U32 => "u32", U64 => "u64", U128 => "u128",
        STRING => "string", OPTION => "option", SEQUENCE => "sequence", ARRAY => "array",
        TUPLE => "tuple", MAP => "map", SET => "set", FINITE32 => "finite32", FINITE64 => "finite64",
        UUID_BITS => "uuid-bits", DECIMAL_NUMERIC => "decimal-numeric", DATE => "date",
        LOCAL_TIME => "local-time", LOCAL_DATETIME => "local-datetime", UTC_INSTANT => "utc-instant",
    }
}

#[cfg(test)]
mod tests {
    use super::{rules, ConstantMembership, Membership};
    use serde_json::json;

    #[test]
    fn complete_membership_trees_preserve_order_and_identity() {
        const A: ConstantMembership = ConstantMembership::custom("example:exact:v1");
        const B: ConstantMembership = ConstantMembership::custom("example:folded:v1");
        const FIRST: ConstantMembership = ConstantMembership {
            id: rules::TUPLE.id,
            parameters: &[A, B],
        };
        const SECOND: ConstantMembership = ConstantMembership {
            id: rules::TUPLE.id,
            parameters: &[B, A],
        };
        const SAME: bool = FIRST.same(&FIRST);
        const CHANGED: bool = FIRST.same(&SECOND);
        assert_eq!(SAME, FIRST.membership() == FIRST.membership());
        assert_eq!(CHANGED, FIRST.membership() == SECOND.membership());
        assert_eq!(
            serde_json::from_value::<Membership>(FIRST.to_json()).unwrap(),
            FIRST.membership()
        );
        assert!(!FIRST.same(&ConstantMembership {
            id: rules::TUPLE.id,
            parameters: &[A]
        }));
    }

    #[test]
    fn malformed_membership_documents_fail_including_nested_ids() {
        for value in [
            json!({"id":"", "parameters":[]}),
            json!({"id":"example:x", "parameters":[{"id":"", "parameters":[]}]}),
            json!({"id":"example:x"}),
            json!({"id":"example:x", "parameters":[], "ignored":true}),
            json!({"id":"example:x", "parameters":[null]}),
        ] {
            assert!(serde_json::from_value::<Membership>(value).is_err());
        }
    }

    #[test]
    #[should_panic(expected = "membership ID must not be empty")]
    fn empty_custom_declarations_are_rejected() {
        ConstantMembership::custom("");
    }
}
