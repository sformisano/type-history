//! Filesystem regressions use real Cargo commands and acknowledged boundaries.
#[cfg(unix)]
mod support;

#[cfg(unix)]
#[test]
fn ledger_transactions_reject_stale_authority_and_preserve_source_inputs() {
    support::run();
}
