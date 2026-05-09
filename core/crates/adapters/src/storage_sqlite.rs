//! `storage_sqlite` — submodules for the `SQLite` storage adapter.
//!
//! This module groups the pieces of the `SQLite` storage adapter that are large
//! enough to live in their own files.  The top-level adapter struct
//! ([`crate::sqlite_storage::SqliteStorage`]) remains in `sqlite_storage.rs`
//! for now; the submodules here provide focused functionality that is tested
//! independently.

pub mod audit;
pub mod locks;
pub mod snapshots;
