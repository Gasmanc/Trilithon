//! Seam test: apply-lock-coordination
//!
//! Contracts under test (mirror seams.md):
//!   - `trilithon_adapters::storage_sqlite::locks::AcquiredLock`
//!   - `trilithon_adapters::storage_sqlite::locks::LockError`

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::disallowed_methods,
    clippy::match_wildcard_for_single_variants
)]
// reason: seam test — panics are the correct failure mode here

mod apply_lock_coordination_seam {
    use trilithon_adapters::storage_sqlite::locks::LockError;

    /// Contract: `LockError::AlreadyHeld` carries a PID.
    #[test]
    fn already_held_carries_holder_pid() {
        let err = LockError::AlreadyHeld { pid: 42 };
        match err {
            LockError::AlreadyHeld { pid } => assert_eq!(pid, 42),
            _ => panic!("unexpected variant"),
        }
    }

    /// Contract: `LockError::Storage` wraps the error message.
    #[test]
    fn storage_error_wraps_message() {
        let err = LockError::Storage("disk full".to_owned());
        assert!(err.to_string().contains("disk full"));
    }
}
