//! Type History package paths and compiler signals.

/// Concrete paths and signal names used by one history frontend.
#[derive(Clone, Copy, Debug)]
pub struct ToolContract {
    /// Cargo subcommand without the `cargo-` prefix.
    pub command: &'static str,
    /// Exact selector name without leading dashes.
    pub selector: &'static str,
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

/// Standalone package authority and export contract.
pub const STANDALONE: ToolContract = ToolContract {
    command: "type-history",
    selector: "type",
    ledger_path: "type-history/schemas.json",
    lock_path: "type-history/.schemas.lock",
    strict_env: "TYPE_HISTORY_REQUIRE_FROZEN",
    export_env: "TYPE_HISTORY_SCHEMA_EXPORT",
    export_marker: "TYPE_HISTORY_SCHEMA_EXPORT_V1\t",
    export_test_filter: "__type_history_",
    export_cfg: "type_history_schema_export",
    // Frozen schema identities outlive package and command names.
    schema_id_prefix: "urn:typehistory:schema:",
    authority_kind: "standalone",
};
