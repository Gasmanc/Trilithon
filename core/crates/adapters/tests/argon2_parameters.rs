//! Assert that hashes produced by `hash_password` embed the RFC 9106
//! first-recommendation Argon2id parameters (`m=19456,t=2,p=1`).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use argon2::password_hash::SaltString;
use rand::rngs::OsRng;
use trilithon_adapters::auth::{ARGON2_M_COST_KIB, ARGON2_P_COST, ARGON2_T_COST, hash_password};

#[test]
fn argon2_parameters_embedded_in_hash() {
    let salt = SaltString::generate(&mut OsRng);
    let encoded = hash_password("hunter2", &salt).expect("hash_password must succeed");

    // The PHC string encodes parameters as `m=<mem>,t=<time>,p=<par>`.
    assert!(
        encoded.contains(&format!("m={ARGON2_M_COST_KIB}")),
        "encoded hash must contain m={ARGON2_M_COST_KIB}: {encoded}"
    );
    assert!(
        encoded.contains(&format!("t={ARGON2_T_COST}")),
        "encoded hash must contain t={ARGON2_T_COST}: {encoded}"
    );
    assert!(
        encoded.contains(&format!("p={ARGON2_P_COST}")),
        "encoded hash must contain p={ARGON2_P_COST}: {encoded}"
    );
    // Must use Argon2id variant.
    assert!(
        encoded.starts_with("$argon2id$"),
        "encoded hash must start with $argon2id$: {encoded}"
    );
}
