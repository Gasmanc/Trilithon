//! Advisory file lock helper.
//!
//! Prevents two Trilithon daemon instances from opening the same database
//! directory concurrently.

use std::fs::File;
use std::path::{Path, PathBuf};

use fs2::FileExt;

/// An exclusive advisory lock on `<dir>/trilithon.lock`.
///
/// The lock is released when this value is dropped.
#[derive(Debug)]
pub struct LockHandle {
    /// The open file that holds the OS-level lock.  Must stay open for the
    /// lock to remain held.
    file: File,
}

impl LockHandle {
    /// Acquire an exclusive advisory lock on `<dir>/trilithon.lock`.
    ///
    /// Uses a **non-blocking** attempt so that a second daemon running in the
    /// same OS process or a different one will see an immediate error rather
    /// than blocking.
    ///
    /// # Errors
    ///
    /// Returns [`LockError::AlreadyHeld`] when the lock is currently held by
    /// another descriptor, or [`LockError::Io`] for underlying filesystem
    /// errors.
    pub fn acquire(dir: &Path) -> Result<Self, LockError> {
        let lock_path = dir.join("trilithon.lock");

        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|source| LockError::Io { source })?;

        file.try_lock_exclusive().map_err(|e| {
            // `fs2` returns `WouldBlock` when the lock is already held.
            if e.kind() == std::io::ErrorKind::WouldBlock {
                LockError::AlreadyHeld {
                    path: lock_path.clone(),
                }
            } else {
                LockError::Io { source: e }
            }
        })?;

        Ok(Self { file })
    }
}

impl Drop for LockHandle {
    fn drop(&mut self) {
        // Best-effort unlock via `fs2::FileExt`; ignore errors on drop.
        let _ = FileExt::unlock(&self.file);
    }
}

/// Errors returned by [`LockHandle::acquire`].
#[derive(Debug, thiserror::Error)]
pub enum LockError {
    /// The lock is already held, likely by another Trilithon daemon.
    #[error("another Trilithon may be running (lock held on {path})")]
    AlreadyHeld {
        /// Path of the lock file that is held.
        path: PathBuf,
    },

    /// An I/O error occurred while creating or locking the file.
    #[error("io error acquiring lock: {source}")]
    Io {
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}
