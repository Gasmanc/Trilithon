//! Adapter from the typed [`trilithon_core::exit::ExitCode`] to
//! [`std::process::ExitCode`].

use trilithon_core::exit::ExitCode as CoreExitCode;

/// Convert a core exit code into the standard library's process exit code.
pub fn to_process_exit(code: CoreExitCode) -> std::process::ExitCode {
    std::process::ExitCode::from(code.as_u8())
}
