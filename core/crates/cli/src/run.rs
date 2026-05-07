//! Daemon run loop with signal handling and graceful shutdown.

use std::sync::Arc;

use tokio::task::JoinSet;
use trilithon_adapters::sqlite_storage::SqliteStorage;
use trilithon_core::config::DaemonConfig;
use trilithon_core::exit::ExitCode;

use crate::exit::caddy_startup_exit_code;
use crate::shutdown::{
    DRAIN_BUDGET, ShutdownController, ShutdownSignal, SignalKind, wait_for_signal,
};

/// Instance ID used to identify this Caddy instance in probes, capability
/// records, and the reconnect loop.  Phase 5 will replace this with a
/// database-backed identifier.
const CADDY_INSTANCE_ID: &str = "local";

/// The placeholder daemon work task.
///
/// In Phase 1 this simply waits for the shutdown signal and returns.
async fn daemon_loop(mut signal: ShutdownSignal) {
    signal.wait().await;
}

/// Ensure the ownership sentinel, emitting a structured warning on takeover.
///
/// Returns `Ok(())` on success (created, already ours, or took over) or
/// `Err(exit_code)` when the sentinel check failed.
async fn check_sentinel(
    caddy_client: &dyn trilithon_core::caddy::client::CaddyClient,
    installation_id: &str,
    takeover: bool,
) -> Result<(), ExitCode> {
    match trilithon_adapters::caddy::sentinel::ensure_sentinel(
        caddy_client,
        installation_id,
        takeover,
    )
    .await
    {
        Err(e) => {
            tracing::error!(error = %e, "caddy.sentinel.failed");
            Err(caddy_startup_exit_code())
        }
        Ok((
            trilithon_adapters::caddy::sentinel::SentinelOutcome::TookOver {
                ref previous_installation_id,
            },
            _,
        )) => {
            // Phase 6 will wire the audit event into persistent storage.
            // Log it here so the takeover is not silently dropped.
            tracing::warn!(
                previous_installation_id = %previous_installation_id,
                new_installation_id = %installation_id,
                "caddy.ownership-sentinel.takeover",
            );
            Ok(())
        }
        Ok(_) => Ok(()),
    }
}

/// Run the daemon until SIGINT or SIGTERM, then drain tasks within the budget.
///
/// `takeover` is forwarded to the ownership-sentinel check so that a foreign
/// sentinel in the running Caddy config can be overwritten rather than
/// aborting with exit code 3.
///
/// # Errors
///
/// Returns an error if OS signal handler installation fails.
pub async fn run_with_shutdown(config: DaemonConfig, takeover: bool) -> anyhow::Result<ExitCode> {
    // Open storage — failure exits 3.
    let storage = SqliteStorage::open(&config.storage.data_dir)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "storage.open.failed");
            anyhow::anyhow!("storage open failed: {e}")
        })?;

    // Apply migrations — failure exits 3.  `apply_migrations` logs
    // `storage.migrations.applied` with version/applied counts on success.
    trilithon_adapters::migrate::apply_migrations(storage.pool())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "migration.failed");
            anyhow::anyhow!("migration failed: {e}")
        })?;

    let pool = storage.pool().clone();

    let (controller, signal) = ShutdownController::new();

    // Collect all background task handles so they can be drained on shutdown.
    let mut tasks: JoinSet<()> = JoinSet::new();

    // Spawn the periodic integrity-check background task.
    tasks.spawn(trilithon_adapters::integrity_check::run_integrity_loop(
        pool.clone(),
        trilithon_adapters::integrity_check::DEFAULT_INTERVAL,
        Box::new(signal.clone()) as Box<dyn trilithon_core::lifecycle::ShutdownObserver>,
    ));

    // Build the Caddy HTTP client.
    let caddy_client = Arc::new(
        trilithon_adapters::caddy::hyper_client::HyperCaddyClient::from_config(
            &config.caddy.admin_endpoint,
            std::time::Duration::from_secs(config.caddy.connect_timeout_seconds.into()),
            std::time::Duration::from_secs(config.caddy.apply_timeout_seconds.into()),
        )
        .map_err(|e| {
            tracing::error!(error = %e, "caddy.client.build-failed");
            anyhow::anyhow!("failed to build Caddy client: {e}")
        })?,
    );

    let cap_cache = Arc::new(trilithon_adapters::caddy::cache::CapabilityCache::default());
    let cap_store = trilithon_adapters::caddy::capability_store::CapabilityStore::new(pool.clone());

    // Run the initial capability probe.
    if let Err(e) = trilithon_adapters::caddy::probe::run_initial_probe(
        &*caddy_client,
        cap_cache.clone(),
        &cap_store,
        CADDY_INSTANCE_ID,
    )
    .await
    {
        tracing::error!(error = %e, "caddy.unreachable");
        return Ok(caddy_startup_exit_code());
    }

    // Read or create the persistent installation id.
    // `read_or_create` does synchronous filesystem I/O; run it off the async
    // executor to avoid blocking the Tokio thread pool.
    let installation_id = tokio::task::spawn_blocking({
        let data_dir = config.storage.data_dir.clone();
        move || trilithon_adapters::caddy::installation_id::read_or_create(&data_dir)
    })
    .await
    .map_err(|e| anyhow::anyhow!("installation-id task panicked: {e}"))?
    .map_err(|e| {
        tracing::error!(error = %e, "installation-id.read-failed");
        anyhow::anyhow!("failed to read/create installation id: {e}")
    })?;

    // Ensure the ownership sentinel.
    if let Err(exit_code) = check_sentinel(&*caddy_client, &installation_id, takeover).await {
        return Ok(exit_code);
    }

    // Spawn the background reconnect loop.
    tasks.spawn(trilithon_adapters::caddy::reconnect::reconnect_loop(
        caddy_client.clone(),
        cap_cache.clone(),
        cap_store,
        CADDY_INSTANCE_ID.into(),
        signal.clone(),
        trilithon_adapters::caddy::reconnect::HEALTH_INTERVAL,
    ));

    // Emit daemon.started only after every startup gate has passed.
    tracing::info!("daemon.started");

    tasks.spawn(daemon_loop(signal));

    // Wait for a Unix signal.  On error (OS refuses handler install), trigger
    // shutdown explicitly so all background tasks exit cleanly before we return.
    let kind = match wait_for_signal().await {
        Ok(k) => k,
        Err(e) => {
            controller.trigger();
            drain_tasks(&mut tasks).await;
            return Err(e);
        }
    };
    match kind {
        SignalKind::Interrupt => {
            tracing::info!(reason = "sigint", "daemon.shutting-down");
        }
        SignalKind::Terminate => {
            tracing::info!(reason = "sigterm", "daemon.shutting-down");
        }
    }

    controller.trigger();
    drain_tasks(&mut tasks).await;

    Ok(ExitCode::CleanShutdown)
}

/// Drain all tasks in `set` within [`DRAIN_BUDGET`], then abort any survivors.
async fn drain_tasks(set: &mut JoinSet<()>) {
    let drained = tokio::time::timeout(DRAIN_BUDGET, async {
        while let Some(result) = set.join_next().await {
            if let Err(e) = result {
                tracing::error!(error = %e, "daemon.task-panicked");
            }
        }
    })
    .await;

    match drained {
        Ok(()) => tracing::info!(forced = false, "daemon.shutdown-complete"),
        Err(_elapsed) => {
            set.abort_all();
            tracing::warn!(forced = true, "daemon.shutdown-complete");
        }
    }
}
