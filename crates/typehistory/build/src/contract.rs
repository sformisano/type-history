//! Exact package paths and compiler signals for the two supported frontends.

/// Concrete paths and signal names used by one history frontend.
#[derive(Clone, Copy, Debug)]
pub struct ToolContract {
    /// Cargo subcommand without the `cargo-` prefix.
    pub command: &'static str,
    /// Exact selector name without leading dashes.
    pub selector: &'static str,
    /// Package-relative configuration path.
    pub config_path: &'static str,
    /// Package-relative committed ledger path.
    pub ledger_path: &'static str,
    /// Package-relative filesystem lock path.
    pub lock_path: &'static str,
    /// Explicit strict build switch.
    pub strict_env: &'static str,
    /// Compiler export switch.
    pub export_env: &'static str,
    /// Prefix identifying generated export records.
    pub export_marker: &'static str,
    /// Test-name filter selecting generated exports.
    pub export_test_filter: &'static str,
    /// Cargo cfg activated for generated export tests.
    pub export_cfg: &'static str,
    /// Exact schema document identity prefix.
    pub schema_id_prefix: &'static str,
    /// Exact macro authority frontend name.
    pub authority_kind: &'static str,
}

/// Existing Axis package authority and export contract.
pub const AXIS: ToolContract = ToolContract {
    command: "axis",
    selector: "event",
    config_path: "axis.toml",
    ledger_path: "axis/event-schemas.json",
    lock_path: "axis/.event-schemas.lock",
    strict_env: "AXIS_REQUIRE_FROZEN_EVENTS",
    export_env: "AXIS_SCHEMA_EXPORT",
    export_marker: "AXIS_SCHEMA_EXPORT_V1\t",
    export_test_filter: "__axis_internal_export_schema_",
    export_cfg: "axis_schema_export",
    schema_id_prefix: "urn:axis:schema:",
    authority_kind: "axis",
};

/// Standalone package authority and export contract.
pub const STANDALONE: ToolContract = ToolContract {
    command: "typehistory",
    selector: "type",
    config_path: "typehistory.toml",
    ledger_path: "typehistory/schemas.json",
    lock_path: "typehistory/.schemas.lock",
    strict_env: "TYPEHISTORY_REQUIRE_FROZEN",
    export_env: "TYPEHISTORY_SCHEMA_EXPORT",
    export_marker: "TYPEHISTORY_SCHEMA_EXPORT_V1\t",
    export_test_filter: "__typehistory_",
    export_cfg: "typehistory_schema_export",
    schema_id_prefix: "urn:typehistory:schema:",
    authority_kind: "standalone",
};
