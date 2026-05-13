//! Property test: 100 random passwords produce 100 distinct hashes.
//!
//! This also verifies that the salt is freshly generated for each call,
//! ensuring even identical passwords produce distinct encoded strings.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::collections::HashSet;

use argon2::password_hash::SaltString;
use rand::Rng;
use rand::rngs::OsRng;
use trilithon_adapters::auth::hash_password;

#[test]
fn argon2_distinct_passwords_produce_distinct_hashes() {
    let mut hashes: HashSet<String> = HashSet::new();

    for i in 0..100 {
        // Use a unique password for each iteration.
        let password = format!(
            "password-iteration-{i}-{}",
            rand::thread_rng().r#gen::<u64>()
        );
        let salt = SaltString::generate(&mut OsRng);
        let encoded = hash_password(&password, &salt).expect("hash_password must succeed");
        let inserted = hashes.insert(encoded.clone());
        assert!(
            inserted,
            "duplicate hash produced for password #{i}: {encoded}"
        );
    }

    assert_eq!(hashes.len(), 100, "expected 100 distinct hashes");
}
