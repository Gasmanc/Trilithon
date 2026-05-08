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

pub use event::{AUDIT_KIND_REGEX, AUDIT_KIND_VOCAB, AuditEvent, AuditEventParseError};
