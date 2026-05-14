//! `UserStore` — typed access to the `users` table via `SQLite`.

use argon2::password_hash::SaltString;
use async_trait::async_trait;
use rand::rngs::OsRng;
use sqlx::Row as _;
use sqlx::SqlitePool;
use ulid::Ulid;

use super::passwords::{PasswordError, hash_password, verify_password};

/// A single user row, without the password hash.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct User {
    /// ULID-encoded primary key.
    pub id: String,
    /// Unique login name.
    pub username: String,
    /// Access role assigned to the user.
    pub role: UserRole,
    /// Unix timestamp (seconds) when the user was created.
    pub created_at: i64,
    /// When `true` the user must change their password on next login.
    pub must_change_pw: bool,
    /// Unix timestamp (seconds) when the user was disabled, or `None` if active.
    pub disabled_at: Option<i64>,
}

/// The roles a user may hold.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    /// Full administrative access.
    Owner,
    /// Operational access; can apply configuration changes.
    Operator,
    /// Read-only access.
    Reader,
}

impl UserRole {
    /// Return the lowercase string representation stored in the database.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Operator => "operator",
            Self::Reader => "reader",
        }
    }
}

impl std::str::FromStr for UserRole {
    type Err = UserStoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "operator" => Ok(Self::Operator),
            "reader" => Ok(Self::Reader),
            other => Err(UserStoreError::UnknownRole(other.to_owned())),
        }
    }
}

/// Errors returned by [`UserStore`] operations.
#[derive(Debug, thiserror::Error)]
pub enum UserStoreError {
    /// A SQLite-level error occurred.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    /// Password hashing or verification failed.
    #[error("password hashing error: {0}")]
    Password(#[from] PasswordError),
    /// The `role` column contained an unrecognised string.
    #[error("unknown role: {0}")]
    UnknownRole(String),
    /// No user with the given id was found.
    #[error("user not found: {0}")]
    NotFound(String),
}

/// Typed access to the `users` table.
#[async_trait]
pub trait UserStore: Send + Sync + 'static {
    /// Look up a user by username; returns the row and its stored password hash.
    ///
    /// # Errors
    ///
    /// Returns [`UserStoreError`] on database or row-mapping failure.
    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(User, String)>, UserStoreError>;

    /// Create a new user with the given role; hashes the password internally.
    ///
    /// # Errors
    ///
    /// Returns [`UserStoreError`] on database failure or password hashing failure.
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
    ) -> Result<User, UserStoreError>;

    /// Re-hash and persist a new password for an existing user.
    ///
    /// # Errors
    ///
    /// Returns [`UserStoreError::NotFound`] if `user_id` does not exist.
    /// Returns [`UserStoreError`] on database or hashing failure.
    async fn update_password(
        &self,
        user_id: &str,
        new_password: &str,
    ) -> Result<(), UserStoreError>;

    /// Set or clear the `must_change_pw` flag for a user.
    ///
    /// # Errors
    ///
    /// Returns [`UserStoreError::NotFound`] if `user_id` does not exist.
    /// Returns [`UserStoreError`] on database failure.
    async fn set_must_change_pw(&self, user_id: &str, value: bool) -> Result<(), UserStoreError>;

    /// Look up a user by their id; returns the row and its stored password hash.
    ///
    /// # Errors
    ///
    /// Returns [`UserStoreError`] on database or row-mapping failure.
    async fn find_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<(User, String)>, UserStoreError>;

    /// Return the total number of user rows in the store.
    ///
    /// # Errors
    ///
    /// Returns [`UserStoreError`] on database failure.
    async fn user_count(&self) -> Result<u64, UserStoreError>;
}

/// SQLite-backed implementation of [`UserStore`].
///
/// Constructed from an existing [`SqlitePool`]; migrations must already
/// have been applied before any methods are called.
pub struct SqliteUserStore {
    pool: SqlitePool,
}

impl SqliteUserStore {
    /// Wrap an existing pool. Migrations must already have been applied.
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn row_to_user_and_hash(row: &sqlx::sqlite::SqliteRow) -> Result<(User, String), UserStoreError> {
    let role_str: String = row.try_get("role").map_err(UserStoreError::Database)?;
    let role: UserRole = role_str.parse()?;
    let must_change_pw_int: i64 = row
        .try_get("must_change_pw")
        .map_err(UserStoreError::Database)?;
    let user = User {
        id: row.try_get("id").map_err(UserStoreError::Database)?,
        username: row.try_get("username").map_err(UserStoreError::Database)?,
        role,
        created_at: row
            .try_get("created_at")
            .map_err(UserStoreError::Database)?,
        must_change_pw: must_change_pw_int != 0,
        disabled_at: row
            .try_get("disabled_at")
            .map_err(UserStoreError::Database)?,
    };
    let hash: String = row
        .try_get("password_hash")
        .map_err(UserStoreError::Database)?;
    Ok((user, hash))
}

#[async_trait]
impl UserStore for SqliteUserStore {
    async fn find_user_by_id(
        &self,
        user_id: &str,
    ) -> Result<Option<(User, String)>, UserStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, username, role, created_at, must_change_pw, disabled_at, password_hash
            FROM users
            WHERE id = ?
            ",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(UserStoreError::Database)?;

        row.map_or_else(|| Ok(None), |r| row_to_user_and_hash(&r).map(Some))
    }

    async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(User, String)>, UserStoreError> {
        let row = sqlx::query(
            r"
            SELECT id, username, role, created_at, must_change_pw, disabled_at, password_hash
            FROM users
            WHERE username = ?
            ",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(UserStoreError::Database)?;

        row.map_or_else(|| Ok(None), |r| row_to_user_and_hash(&r).map(Some))
    }

    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role: UserRole,
    ) -> Result<User, UserStoreError> {
        let id = Ulid::new().to_string();
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = hash_password(password, &salt)?;
        let role_str = role.as_str();
        // Seconds since Unix epoch; i64 overflows in year 2262, acceptable.
        #[allow(clippy::cast_possible_wrap)]
        // zd:phase-09 expires:2027-01-01 reason: unix secs fit in i64 until year 2262
        let now = std::time::SystemTime::UNIX_EPOCH
            .elapsed()
            .map_or(0_i64, |d| d.as_secs() as i64);

        sqlx::query(
            r"
            INSERT INTO users (id, username, password_hash, role, created_at, must_change_pw)
            VALUES (?, ?, ?, ?, ?, 0)
            ",
        )
        .bind(&id)
        .bind(username)
        .bind(&password_hash)
        .bind(role_str)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(UserStoreError::Database)?;

        Ok(User {
            id,
            username: username.to_owned(),
            role,
            created_at: now,
            must_change_pw: false,
            disabled_at: None,
        })
    }

    async fn update_password(
        &self,
        user_id: &str,
        new_password: &str,
    ) -> Result<(), UserStoreError> {
        let salt = SaltString::generate(&mut OsRng);
        let password_hash = hash_password(new_password, &salt)?;

        let result = sqlx::query(r"UPDATE users SET password_hash = ? WHERE id = ?")
            .bind(&password_hash)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(UserStoreError::Database)?;

        if result.rows_affected() == 0 {
            return Err(UserStoreError::NotFound(user_id.to_owned()));
        }
        Ok(())
    }

    async fn set_must_change_pw(&self, user_id: &str, value: bool) -> Result<(), UserStoreError> {
        let flag = i64::from(value);
        let result = sqlx::query(r"UPDATE users SET must_change_pw = ? WHERE id = ?")
            .bind(flag)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(UserStoreError::Database)?;

        if result.rows_affected() == 0 {
            return Err(UserStoreError::NotFound(user_id.to_owned()));
        }
        Ok(())
    }

    async fn user_count(&self) -> Result<u64, UserStoreError> {
        let row = sqlx::query(r"SELECT COUNT(*) AS cnt FROM users")
            .fetch_one(&self.pool)
            .await
            .map_err(UserStoreError::Database)?;
        let count: i64 = row.try_get("cnt").map_err(UserStoreError::Database)?;
        Ok(count.unsigned_abs())
    }
}

/// Verify a password against a stored hash.
///
/// # Errors
///
/// Returns [`PasswordError`] if `encoded_hash` is not valid or the Argon2 library fails.
pub fn check_password(plaintext: &str, encoded_hash: &str) -> Result<bool, PasswordError> {
    verify_password(plaintext, encoded_hash)
}
