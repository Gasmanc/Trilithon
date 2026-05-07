//! Typed exit codes for the Trilithon daemon.
//!
//! These values are part of the public contract of the binary. Do **not**
//! renumber them without bumping the major version and updating docs.

/// Well-known exit codes returned by the Trilithon CLI.
///
/// The numeric values are stable and documented in the man page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExitCode {
    /// The process completed without error.
    CleanShutdown = 0,
    /// A configuration error was detected.
    ConfigError = 2,
    /// A required pre-condition was not met at startup.
    StartupPreconditionFailure = 3,
    /// The command was invoked incorrectly.
    InvalidInvocation = 64,
    /// An unexpected panic occurred in a background runtime task.
    RuntimePanic = 70,
}

impl ExitCode {
    /// Returns the exit code as a raw `u8`.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl From<crate::storage::StorageError> for ExitCode {
    fn from(_: crate::storage::StorageError) -> Self {
        Self::StartupPreconditionFailure
    }
}

#[cfg(test)]
mod tests {
    use super::ExitCode;

    #[test]
    fn values_are_stable() {
        assert_eq!(ExitCode::CleanShutdown as u8, 0);
        assert_eq!(ExitCode::ConfigError as u8, 2);
        assert_eq!(ExitCode::StartupPreconditionFailure as u8, 3);
        assert_eq!(ExitCode::InvalidInvocation as u8, 64);
        assert_eq!(ExitCode::RuntimePanic as u8, 70);
    }
}
