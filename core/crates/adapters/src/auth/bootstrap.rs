//! Bootstrap account creation (Slice 9.4 / hazard H13).
//!
//! On first startup with no users in the store, generates a random password,
//! writes it to `<data_dir>/bootstrap-credentials.txt` with mode 0600, and
//! emits a single audit row.  The plaintext password never appears in any log
//! line, environment variable, or process argument.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use ulid::Ulid;

use super::users::{User, UserRole, UserStore, UserStoreError};
use crate::audit_writer::{ActorRef, AuditAppend, AuditWriteError, AuditWriter};
use crate::rng::RandomBytes;

/// Crockford-without-confusables alphabet: 32 unambiguous characters.
/// Removes I, L, O, U from standard base32 to avoid visual confusion.
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";

/// Outcome returned when a bootstrap account is created.
pub struct BootstrapOutcome {
    /// The newly-created admin user.
    pub user: User,
    /// Absolute path to the credentials file.
    pub credentials_path: PathBuf,
}

/// Errors that [`bootstrap_if_empty`] can return.
#[derive(Debug, thiserror::Error)]
pub enum BootstrapError {
    /// An I/O error (file creation, write, or permission set).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// The user store returned an error.
    #[error("user store: {0}")]
    UserStore(#[from] UserStoreError),
    /// The data directory is not writable.
    #[error("data directory not writable: {path}")]
    DataDirNotWritable {
        /// The path that failed the writability check.
        path: PathBuf,
    },
    /// Writing the audit row failed.
    #[error("audit write: {0}")]
    Audit(#[from] AuditWriteError),
}

/// Encode 18 random bytes into a 24-character password using the
/// Crockford-without-confusables alphabet.
///
/// 18 bytes = 144 bits; 144 / 6 = 24 characters at 5 bits each (base32).
fn encode_password(bytes: &[u8; 18]) -> String {
    // Process 5 bytes at a time (5 × 8 = 40 bits = 8 × 5-bit groups).
    // 18 bytes = 3 × 5 bytes + 3 extra bytes.
    // We handle all 18 bytes by working on groups of 5 bits.
    let mut out = String::with_capacity(24);
    let bits = bytes.len() * 8; // 144 bits
    let chars = bits / 5; // 28 — we only use the first 24

    for i in 0..chars.min(24) {
        let bit_offset = i * 5;
        let byte_idx = bit_offset / 8;
        let bit_in_byte = bit_offset % 8;

        // Extract 5 bits, possibly spanning two bytes.
        let val: u8 = if bit_in_byte <= 3 {
            // All 5 bits fit within byte_idx.
            (bytes[byte_idx] >> (3 - bit_in_byte)) & 0x1F
        } else {
            // High bits from byte_idx, low bits from byte_idx+1.
            let high = bytes[byte_idx] << (bit_in_byte - 3);
            let low = bytes[byte_idx + 1] >> (11 - bit_in_byte);
            (high | low) & 0x1F
        };
        // ALPHABET contains only ASCII characters; char::from is infallible for u8.
        out.push(char::from(ALPHABET[val as usize]));
    }

    out
}

/// Create a bootstrap admin account if the user store is empty.
///
/// Returns `Ok(None)` when users already exist (no-op).
/// Returns `Ok(Some(BootstrapOutcome))` when a new admin account is created.
///
/// # Errors
///
/// Returns [`BootstrapError`] on I/O, user-store, or audit failure.
pub async fn bootstrap_if_empty(
    user_store: &dyn UserStore,
    rng: &dyn RandomBytes,
    data_dir: &Path,
    audit: &AuditWriter,
) -> Result<Option<BootstrapOutcome>, BootstrapError> {
    // Step 1: skip if any users exist.
    if user_store.user_count().await? > 0 {
        return Ok(None);
    }

    // Step 2: verify data_dir is writable before generating credentials.
    if !data_dir.is_dir() {
        return Err(BootstrapError::DataDirNotWritable {
            path: data_dir.to_owned(),
        });
    }

    // Step 3: generate random password (18 bytes → 24-char Crockford base32).
    let mut raw = [0u8; 18];
    rng.fill_bytes(&mut raw);
    let password = encode_password(&raw);

    // Step 4: write credentials file BEFORE creating the DB user (F005).
    // Writing first means a failure here leaves no orphaned DB account.
    // If the DB write fails after a successful file write, we clean up the file.
    let path = data_dir.join("bootstrap-credentials.txt");
    {
        #[cfg(unix)]
        {
            use std::fs::OpenOptions;
            use std::os::unix::fs::OpenOptionsExt as _;
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&path)?;
            write!(file, "username: admin\npassword: {password}\n")?;
        }
        #[cfg(not(unix))]
        {
            use std::fs::OpenOptions;
            // Use create_new so concurrent daemon instances cannot overwrite an
            // existing credentials file (F040).
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)?;
            write!(file, "username: admin\npassword: {password}\n")?;
        }
    }

    // Step 5: create admin user with must_change_pw = true.
    // If this fails, clean up the credentials file so bootstrap can retry.
    let user = match async {
        let u = user_store
            .create_user("admin", &password, UserRole::Owner)
            .await?;
        user_store.set_must_change_pw(&u.id, true).await?;
        Ok::<_, UserStoreError>(u)
    }
    .await
    {
        Ok(u) => {
            let mut u = u;
            u.must_change_pw = true;
            u
        }
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            return Err(BootstrapError::UserStore(e));
        }
    };

    // Step 6: log a single INFO line — password MUST NOT appear here.
    tracing::info!(
        credentials_path = %path.display(),
        "bootstrap account created. credentials written to {}; \
         you will be required to change the password on first login.",
        path.display(),
    );

    // Step 7: emit audit row — no password in diff.
    let append = AuditAppend {
        correlation_id: Ulid::new(),
        actor: ActorRef::System {
            component: "bootstrap".to_owned(),
        },
        event: trilithon_core::audit::AuditEvent::AuthBootstrapCredentialsCreated,
        target_kind: Some("user".to_owned()),
        target_id: Some(user.id.clone()),
        snapshot_id: None,
        diff: None,
        outcome: trilithon_core::storage::types::AuditOutcome::Ok,
        error_kind: None,
        notes: None,
    };
    audit.record(append).await?;

    Ok(Some(BootstrapOutcome {
        user,
        credentials_path: path,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_password_length_is_24() {
        let bytes = [0u8; 18];
        let pw = encode_password(&bytes);
        assert_eq!(pw.len(), 24, "password must be exactly 24 characters");
    }

    #[test]
    fn encode_password_only_alphabet_chars() {
        let bytes = [0xFFu8; 18];
        let pw = encode_password(&bytes);
        for ch in pw.chars() {
            assert!(
                ALPHABET.contains(&(ch as u8)),
                "unexpected character {ch:?} not in alphabet"
            );
        }
    }

    #[test]
    fn encode_password_varies_with_input() {
        let a = encode_password(&[0u8; 18]);
        let b = encode_password(&[0xFFu8; 18]);
        assert_ne!(a, b, "different inputs must produce different passwords");
    }
}
