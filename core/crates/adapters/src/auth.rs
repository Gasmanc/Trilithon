//! Authentication adapters: password hashing, user persistence, sessions,
//! cookie codec, and login rate limiting.

pub mod passwords;
pub mod rate_limit;
pub mod sessions;
pub mod users;

pub use passwords::{
    ARGON2_M_COST_KIB, ARGON2_P_COST, ARGON2_T_COST, PasswordError, argon2id, hash_password,
    verify_password,
};
pub use rate_limit::{LoginRateLimiter, RateLimited};
pub use sessions::{
    Session, SessionError, SessionStore, SqliteSessionStore, build_cookie, parse_cookie,
};
pub use users::{SqliteUserStore, User, UserRole, UserStore, UserStoreError};
