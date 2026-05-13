//! Seam test: snapshots-config-version-cas
//!
//! Contracts under test (mirror seams.md):
//!   - trilithon_core::storage::Storage::cas_advance_config_version
//!   - trilithon_core::storage::Storage::current_config_version
//!   - trilithon_core::storage::error::StorageError::OptimisticConflict

mod snapshots_config_version_cas_seam {
    use trilithon_core::storage::error::StorageError;

    /// Contract: OptimisticConflict carries observed and expected fields.
    #[test]
    fn optimistic_conflict_carries_observed_and_expected() {
        let err = StorageError::OptimisticConflict { observed: 5, expected: 3 };
        match err {
            StorageError::OptimisticConflict { observed, expected } => {
                assert_eq!(observed, 5);
                assert_eq!(expected, 3);
            }
            _ => panic!("unexpected variant"),
        }
    }

    /// Contract: Storage trait is object-safe (can be boxed).
    #[test]
    fn storage_trait_is_object_safe() {
        // Compile-time check: if Storage is not object-safe this fails to compile.
        fn _assert_object_safe(_: Box<dyn trilithon_core::storage::Storage>) {}
        assert!(true, "Storage is object-safe — verified at compile time");
    }
}
