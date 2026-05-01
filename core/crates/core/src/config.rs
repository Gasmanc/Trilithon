// zd:CONFIG-NO-IO expires:2027-04-30 reason:enforced by Cargo.toml manifest review

//! Configuration types for the Trilithon daemon.
//!
//! All types are pure data — no I/O, no async, no filesystem access.

pub mod types;
pub use types::*;
