// Ledger Module - Immutable Audit Trail
//!
//! The ledger is git-backed and IMMUTABLE. It provides an audit trail
//! that the agent cannot modify.

pub mod git_ledger;

pub use git_ledger::{GitLedger, LedgerEntry};
