//! Seam test: apply-lock-coordination
//!
//! Contracts under test (mirror seams.md):
//!   - trilithon_adapters::storage_sqlite::locks::AcquiredLock
//!   - trilithon_adapters::storage_sqlite::locks::LockError

mod apply_lock_coordination_seam {
    use trilithon_adapters::storage_sqlite::locks::LockError;

    /// Contract: LockError::AlreadyHeld carries a PID.
    #[test]
    fn already_held_carries_holder_pid() {
        let err = LockError::AlreadyHeld { pid: 42 };
        match err {
            LockError::AlreadyHeld { pid } => assert_eq!(pid, 42),
            _ => panic!("unexpected variant"),
        }
    }

    /// Contract: LockError::Storage wraps the error message.
    #[test]
    fn storage_error_wraps_message() {
        let err = LockError::Storage("disk full".to_owned());
        assert!(err.to_string().contains("disk full"));
    }
}
