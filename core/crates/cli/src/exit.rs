//! Adapter from the typed [`trilithon_core::exit::ExitCode`] to
//! [`std::process::ExitCode`].
//!
//! Also provides helpers that map Caddy-startup errors to the correct exit
//! code so that `run.rs` does not have to repeat the mapping logic.

use trilithon_core::exit::ExitCode as CoreExitCode;

/// Convert a core exit code into the standard library's process exit code.
pub fn to_process_exit(code: CoreExitCode) -> std::process::ExitCode {
    std::process::ExitCode::from(code.as_u8())
}

/// Return the exit code for any Caddy startup precondition failure.
///
/// Both [`trilithon_adapters::caddy::sentinel::SentinelError::Conflict`] and
/// [`trilithon_adapters::caddy::validate_endpoint::EndpointPolicyError::NonLoopback`]
/// map to [`CoreExitCode::StartupPreconditionFailure`] (exit 3).
pub const fn caddy_startup_exit_code() -> CoreExitCode {
    CoreExitCode::StartupPreconditionFailure
}
