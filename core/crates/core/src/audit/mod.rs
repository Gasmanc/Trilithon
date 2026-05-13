//! Audit subsystem — pure-core type machinery.
//!
//! This module declares the closed Tier 1 audit-event vocabulary
//! (architecture §6.6) and the types consumed by later Phase 6 slices.
//!
//! - [`event`] — [`AuditEvent`] enum, `Display`/`FromStr`, `AUDIT_KIND_REGEX`.
//! - `row` (Slice 6.2) — `AuditEventRow`, the storable record shape.
//! - `redactor` (Slice 6.3) — field-level redaction logic.

#![allow(clippy::mod_module_files)]

pub mod event;
pub mod redactor;
pub mod row;

pub use event::{
    AUDIT_KIND_REGEX, AUDIT_KIND_VOCAB, AuditEvent, AuditEventParseError,
    validate_audit_kind_pattern,
};
pub use row::{
    AUDIT_QUERY_DEFAULT_LIMIT, AUDIT_QUERY_MAX_LIMIT, ActorRef, AuditEventRow, AuditOutcome,
    AuditRowId, AuditSelector, NormalisedAuditSelector,
};
