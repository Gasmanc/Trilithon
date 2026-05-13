//! Authentication adapters: password hashing and user persistence.

pub mod passwords;
pub mod users;

pub use passwords::{
    ARGON2_M_COST_KIB, ARGON2_P_COST, ARGON2_T_COST, PasswordError, argon2id, hash_password,
    verify_password,
};
pub use users::{SqliteUserStore, User, UserRole, UserStore, UserStoreError};
