//! Real consumers built from a copy containing only the Type History crate family.

#[path = "standalone/authoring.rs"]
mod authoring;
#[path = "standalone/authority.rs"]
mod authority;
#[path = "standalone/hygiene.rs"]
mod hygiene;
#[path = "standalone/lifecycle.rs"]
mod lifecycle;
#[path = "standalone/source_discovery.rs"]
mod source_discovery;
#[path = "standalone/stable_name.rs"]
mod stable_name;
#[path = "standalone/support.rs"]
mod support;
#[path = "standalone/tutorial.rs"]
mod tutorial;
#[path = "standalone/versioned.rs"]
mod versioned;

#[cfg(feature = "redactable")]
#[path = "standalone/redaction.rs"]
mod redaction;
