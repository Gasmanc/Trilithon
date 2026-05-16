//! Daemon run loop with signal handling and graceful shutdown.

use std::sync::Arc;

use tokio::task::JoinSet;
use trilithon_adapters::sqlite_storage::SqliteStorage;
use trilithon_core::config::DaemonConfig;
use trilithon_core::exit::ExitCode;
use trilithon_core::http::HttpServer as _;

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

/// Run `PRAGMA integrity_check` once at startup (ADR-0006).
///
/// Returns `Ok(())` when `SQLite` reports `ok`, or an error that maps to exit 3.
async fn run_startup_integrity_check(
    pool: &trilithon_adapters::sqlite_storage::SqliteStorage,
) -> anyhow::Result<()> {
    use trilithon_adapters::integrity_check::{IntegrityResult, integrity_check_once};
    match integrity_check_once(pool.pool()).await {
        Ok(IntegrityResult::Ok) => {
            tracing::info!("storage.integrity_check.startup.ok");
            Ok(())
        }
        Ok(IntegrityResult::Failed { detail }) => {
            tracing::error!(detail = %detail, "storage.integrity_check.startup.failed");
            Err(anyhow::anyhow!("startup integrity check failed: {detail}"))
        }
        Err(e) => {
            tracing::error!(error = %e, "storage.integrity_check.startup.error");
            Err(anyhow::anyhow!("startup integrity check error: {e}"))
        }
    }
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
    let storage = open_and_migrate_storage(&config).await?;
    let pool = storage.pool().clone();
    let (controller, signal) = ShutdownController::new();

    // Collect all background task handles so they can be drained on shutdown.
    //
    // Correlation-span wrapping (Slice 6.7 / Phase 6 review F021): each
    // background loop is responsible for opening
    // `with_correlation_span(Ulid::new(), "system", <component>, fut)` once
    // per iteration so individual audit rows can be traced back to a single
    // tick of work.  The wrapping lives inside each loop's own function (e.g.
    // `integrity_check::run_integrity_loop`, `reconnect::reconnect_loop`,
    // drift-detector `run`), not here, because the outer wrapper would tag
    // every iteration with the same id — the opposite of what audit-trail
    // forensics needs.  The HTTP middleware (Phase 9) wraps inbound requests.
    let mut tasks: JoinSet<()> = JoinSet::new();

    // Spawn the periodic integrity-check background task.
    tasks.spawn(trilithon_adapters::integrity_check::run_integrity_loop(
        pool.clone(),
        trilithon_adapters::integrity_check::DEFAULT_INTERVAL,
        Box::new(signal.clone()) as Box<dyn trilithon_core::lifecycle::ShutdownObserver>,
    ));

    // Build the Caddy client, probe Caddy, assert the ownership sentinel,
    // and spawn the background reconnect loop.  Returns `Err(ExitCode)` on
    // any fatal startup condition so the outer function can return early.
    let (caddy_client, apply_mutex, cap_cache) =
        match setup_caddy(&config, &pool, &mut tasks, signal.clone(), takeover).await {
            Ok(triple) => triple,
            Err(code) => return Ok(code),
        };

    // Wrap storage in Arc<dyn Storage> for shared use (bootstrap + drift detector).
    let storage_arc: Arc<dyn trilithon_core::storage::Storage> = Arc::new(storage);

    // Bootstrap admin account on first startup (Slice 9.4).
    {
        let user_store = trilithon_adapters::auth::users::SqliteUserStore::new(pool.clone());
        run_bootstrap(&config, &user_store, Arc::clone(&storage_arc)).await?;
    }

    // Build and spawn the drift detector (Slice 8.5).
    let detector = build_drift_detector(
        Arc::clone(&storage_arc),
        caddy_client.clone(),
        Arc::clone(&apply_mutex),
    );
    if let Err(e) = detector.init_from_storage().await {
        tracing::warn!(error = %e, "drift-detector.init-from-storage-failed");
    }
    let detector_task = Arc::clone(&detector);
    let shutdown_rx = signal.subscribe();
    tasks.spawn(async move {
        detector_task.run(shutdown_rx).await;
    });

    // Build the production CaddyApplier wired to real storage and the Caddy client.
    let applier = {
        let clock: Arc<dyn trilithon_core::clock::Clock> =
            Arc::new(trilithon_core::clock::SystemClock);
        let registry = Arc::new(trilithon_core::schema::SchemaRegistry::with_tier1_secrets());
        let hasher: Arc<dyn trilithon_core::audit::redactor::CiphertextHasher> =
            Arc::new(trilithon_adapters::Sha256AuditHasher);
        let audit = Arc::new(trilithon_adapters::AuditWriter::new_with_arcs(
            Arc::clone(&storage_arc),
            Arc::clone(&clock),
            Arc::clone(&registry),
            Arc::clone(&hasher),
        ));
        Arc::new(trilithon_adapters::CaddyApplier {
            client: Arc::clone(&caddy_client) as Arc<dyn trilithon_core::caddy::CaddyClient>,
            renderer: Arc::new(trilithon_core::reconciler::DefaultCaddyJsonRenderer),
            diff_engine: Arc::new(trilithon_core::diff::NoOpDiffEngine),
            capabilities: Arc::clone(&cap_cache),
            audit,
            storage: Arc::clone(&storage_arc),
            instance_id: CADDY_INSTANCE_ID.to_owned(),
            clock,
            instance_mutex: Arc::clone(&apply_mutex),
            lock_pool: pool.clone(),
            tls_observer: None,
        })
    };

    // Build and spawn the HTTP server (Slice 9.1).
    let ready_since_ms = match bind_and_spawn_http(
        &config,
        &mut tasks,
        signal.clone(),
        Arc::clone(&storage_arc),
        Arc::clone(&detector),
        Arc::clone(&cap_cache),
        applier,
        pool.clone(),
    )
    .await
    {
        Ok(r) => r,
        Err(code) => return Ok(code),
    };

    // Emit daemon.started only after every startup gate has passed.
    tracing::info!("daemon.started");
    trilithon_adapters::http_axum::mark_ready(&ready_since_ms);
    tasks.spawn(daemon_loop(signal));

    // Wait for a Unix signal.  On error (OS refuses handler install), trigger
    // shutdown explicitly so all background tasks exit cleanly before we return.
    let kind = match wait_for_signal().await {
        Ok(k) => k,
        Err(e) => {
            controller.trigger();
            let _ = drain_tasks(&mut tasks).await;
            return Err(e);
        }
    };
    match kind {
        SignalKind::Interrupt => tracing::info!(reason = "sigint", "daemon.shutting-down"),
        SignalKind::Terminate => tracing::info!(reason = "sigterm", "daemon.shutting-down"),
    }

    controller.trigger();
    let panicked = drain_tasks(&mut tasks).await;
    Ok(if panicked {
        ExitCode::RuntimePanic
    } else {
        ExitCode::CleanShutdown
    })
}

/// Build the Caddy HTTP client, run the initial capability probe, assert the
/// ownership sentinel, and spawn the background reconnect loop.
///
/// Returns `Ok((caddy_client, apply_mutex))` on success, or
/// `Err(ExitCode)` when a fatal startup condition is encountered.
async fn setup_caddy(
    config: &DaemonConfig,
    pool: &trilithon_adapters::sqlite_storage::SqlitePool,
    tasks: &mut JoinSet<()>,
    signal: crate::shutdown::ShutdownSignal,
    takeover: bool,
) -> Result<
    (
        Arc<trilithon_adapters::caddy::hyper_client::HyperCaddyClient>,
        Arc<tokio::sync::Mutex<()>>,
        Arc<trilithon_adapters::caddy::cache::CapabilityCache>,
    ),
    ExitCode,
> {
    let caddy_client = Arc::new(
        trilithon_adapters::caddy::hyper_client::HyperCaddyClient::from_config(
            &config.caddy.admin_endpoint,
            std::time::Duration::from_secs(config.caddy.connect_timeout_seconds.into()),
            std::time::Duration::from_secs(config.caddy.apply_timeout_seconds.into()),
        )
        .map_err(|e| {
            tracing::error!(error = %e, "caddy.client.build-failed");
            caddy_startup_exit_code()
        })?,
    );

    let cap_cache = Arc::new(trilithon_adapters::caddy::cache::CapabilityCache::default());
    let cap_store = trilithon_adapters::caddy::capability_store::CapabilityStore::new(pool.clone());

    if let Err(e) = trilithon_adapters::caddy::probe::run_initial_probe(
        &*caddy_client,
        cap_cache.clone(),
        &cap_store,
        CADDY_INSTANCE_ID,
    )
    .await
    {
        tracing::error!(error = %e, "caddy.unreachable");
        return Err(caddy_startup_exit_code());
    }

    let installation_id = tokio::task::spawn_blocking({
        let data_dir = config.storage.data_dir.clone();
        move || trilithon_adapters::caddy::installation_id::read_or_create(&data_dir)
    })
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "installation-id.task-panicked");
        caddy_startup_exit_code()
    })?
    .map_err(|e| {
        tracing::error!(error = %e, "installation-id.read-failed");
        caddy_startup_exit_code()
    })?;

    check_sentinel(&*caddy_client, &installation_id, takeover).await?;

    tasks.spawn(trilithon_adapters::caddy::reconnect::reconnect_loop(
        caddy_client.clone(),
        cap_cache.clone(),
        cap_store,
        CADDY_INSTANCE_ID.into(),
        signal,
        trilithon_adapters::caddy::reconnect::HEALTH_INTERVAL,
    ));

    let apply_mutex: Arc<tokio::sync::Mutex<()>> = Arc::new(tokio::sync::Mutex::new(()));
    Ok((caddy_client, apply_mutex, cap_cache))
}

/// Open the `SQLite` store, run migrations, verify `application_id`, and run the
/// startup integrity check.
async fn open_and_migrate_storage(config: &DaemonConfig) -> anyhow::Result<SqliteStorage> {
    let storage = SqliteStorage::open(&config.storage.data_dir)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "storage.open.failed");
            anyhow::anyhow!("storage open failed: {e}")
        })?;
    trilithon_adapters::migrate::apply_migrations(storage.pool())
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "migration.failed");
            anyhow::anyhow!("migration failed: {e}")
        })?;
    storage.verify_application_id().await.map_err(|e| {
        tracing::error!(error = %e, "storage.application_id.mismatch");
        anyhow::anyhow!("application_id check failed: {e}")
    })?;
    run_startup_integrity_check(&storage).await?;
    Ok(storage)
}

/// Construct the [`DriftDetector`] with its audit writer and shared apply mutex.
///
/// `apply_mutex` must be the same `Arc` passed to `CaddyApplier` so that
/// `SkippedApplyInFlight` can trigger correctly when a config-write is active.
fn build_drift_detector(
    storage: Arc<dyn trilithon_core::storage::Storage>,
    caddy_client: Arc<trilithon_adapters::caddy::hyper_client::HyperCaddyClient>,
    apply_mutex: Arc<tokio::sync::Mutex<()>>,
) -> Arc<trilithon_adapters::drift::DriftDetector> {
    let drift_storage = storage;
    let drift_clock: Arc<dyn trilithon_core::clock::Clock> =
        Arc::new(trilithon_core::clock::SystemClock);
    let drift_registry = Arc::new(trilithon_core::schema::SchemaRegistry::with_tier1_secrets());
    let drift_hasher: Arc<dyn trilithon_core::audit::redactor::CiphertextHasher> =
        Arc::new(trilithon_adapters::Sha256AuditHasher);
    let drift_audit = Arc::new(trilithon_adapters::AuditWriter::new_with_arcs(
        drift_storage.clone(),
        drift_clock.clone(),
        drift_registry,
        drift_hasher,
    ));
    let drift_config = trilithon_adapters::drift::DriftDetectorConfig {
        interval: std::time::Duration::from_secs(60),
        instance_id: CADDY_INSTANCE_ID.to_owned(),
    };
    Arc::new(trilithon_adapters::drift::DriftDetector {
        config: drift_config,
        client: caddy_client,
        renderer: Arc::new(trilithon_core::reconciler::DefaultCaddyJsonRenderer),
        storage: drift_storage,
        audit: drift_audit,
        clock: drift_clock,
        apply_mutex,
        last_running_hash: tokio::sync::Mutex::new(None),
    })
}

/// Bind the HTTP server from `config`, spawn it into `tasks`, and return the
/// `ready_since_unix_ms` atomic so the caller can mark the daemon ready.
///
/// Returns `Err(ExitCode)` if the bind fails.
#[allow(clippy::too_many_arguments)]
async fn bind_and_spawn_http(
    config: &DaemonConfig,
    tasks: &mut JoinSet<()>,
    signal: crate::shutdown::ShutdownSignal,
    storage: Arc<dyn trilithon_core::storage::trait_def::Storage>,
    drift_detector: Arc<trilithon_adapters::drift::DriftDetector>,
    capability_cache: Arc<trilithon_adapters::caddy::cache::CapabilityCache>,
    applier: Arc<trilithon_adapters::CaddyApplier>,
    pool: trilithon_adapters::sqlite_storage::SqlitePool,
) -> Result<Arc<std::sync::atomic::AtomicU64>, ExitCode> {
    use std::sync::atomic::{AtomicBool, AtomicU64};

    let apply_in_flight_flag = Arc::new(AtomicBool::new(false));
    let ready_since_ms = Arc::new(AtomicU64::new(0));

    let clock: Arc<dyn trilithon_core::clock::Clock> = Arc::new(trilithon_core::clock::SystemClock);
    let schema_registry = Arc::new(trilithon_core::schema::SchemaRegistry::with_tier1_secrets());
    let hasher: Arc<dyn trilithon_core::audit::redactor::CiphertextHasher> =
        Arc::new(trilithon_adapters::Sha256AuditHasher);
    let audit_writer = Arc::new(trilithon_adapters::AuditWriter::new_with_arcs(
        Arc::clone(&storage),
        Arc::clone(&clock),
        Arc::clone(&schema_registry),
        Arc::clone(&hasher),
    ));
    let user_store: Arc<dyn trilithon_adapters::auth::UserStore> = Arc::new(
        trilithon_adapters::auth::users::SqliteUserStore::new(pool.clone()),
    );
    let session_rng: Arc<dyn trilithon_adapters::rng::RandomBytes> =
        Arc::new(trilithon_adapters::rng::ThreadRng);
    let session_store: Arc<dyn trilithon_adapters::auth::SessionStore> = Arc::new(
        trilithon_adapters::auth::sessions::SqliteSessionStore::new(pool.clone(), session_rng),
    );
    let http_state = Arc::new(trilithon_adapters::http_axum::AppState {
        apply_in_flight: Arc::clone(&apply_in_flight_flag),
        ready_since_unix_ms: Arc::clone(&ready_since_ms),
        rate_limiter: Arc::new(trilithon_adapters::auth::rate_limit::LoginRateLimiter::new()),
        session_store,
        user_store,
        audit_writer,
        session_cookie_name: "trilithon_session".to_owned(),
        session_ttl_seconds: 12 * 3600,
        token_pool: Some(pool),
        applier,
        storage,
        diff_engine: Arc::new(trilithon_core::diff::DefaultDiffEngine),
        schema_registry,
        hasher,
        drift_detector,
        capability_cache,
        // Enable Secure cookie flag when binding is not loopback-only (F008).
        secure_cookies: config.server.allow_remote,
        // Trust X-Forwarded-For only when behind a declared reverse proxy (F009).
        trusted_proxy: false,
    });
    let http_server_cfg = trilithon_adapters::http_axum::AxumServerConfig {
        bind_host: config.server.bind.ip().to_string(),
        bind_port: config.server.bind.port(),
        allow_remote_binding: config.server.allow_remote,
        ..trilithon_adapters::http_axum::AxumServerConfig::default()
    };
    let mut http_server =
        trilithon_adapters::http_axum::AxumServer::new(http_server_cfg, http_state);

    match http_server.bind(&config.server).await {
        Ok(addr) => {
            tracing::info!(bind = %addr, "http.server.bound");
        }
        Err(e) => {
            tracing::error!(error = %e, "http.server.bind-failed");
            return Err(ExitCode::StartupPreconditionFailure);
        }
    }

    let http_shutdown = Box::pin({
        let mut s = signal;
        async move {
            s.wait().await;
        }
    });
    tasks.spawn(async move {
        if let Err(e) = http_server.run(http_shutdown).await {
            tracing::error!(error = %e, "http.server.crashed");
        }
    });

    Ok(ready_since_ms)
}

/// Create the bootstrap admin account on first startup if no users exist.
///
/// Delegates to [`trilithon_adapters::auth::bootstrap::bootstrap_if_empty`].
async fn run_bootstrap(
    config: &DaemonConfig,
    user_store: &trilithon_adapters::auth::users::SqliteUserStore,
    storage: Arc<dyn trilithon_core::storage::Storage>,
) -> anyhow::Result<()> {
    let clock: Arc<dyn trilithon_core::clock::Clock> = Arc::new(trilithon_core::clock::SystemClock);
    let registry = Arc::new(trilithon_core::schema::SchemaRegistry::with_tier1_secrets());
    let hasher: Arc<dyn trilithon_core::audit::redactor::CiphertextHasher> =
        Arc::new(trilithon_adapters::Sha256AuditHasher);
    let audit = trilithon_adapters::AuditWriter::new_with_arcs(storage, clock, registry, hasher);

    let rng = trilithon_adapters::rng::ThreadRng;

    trilithon_adapters::auth::bootstrap::bootstrap_if_empty(
        user_store,
        &rng,
        &config.storage.data_dir,
        &audit,
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "bootstrap.failed");
        anyhow::anyhow!("bootstrap failed: {e}")
    })?;

    Ok(())
}

/// Drain all tasks in `set` within [`DRAIN_BUDGET`], then abort any survivors.
///
/// Returns `true` if any task panicked.
async fn drain_tasks(set: &mut JoinSet<()>) -> bool {
    let result = tokio::time::timeout(DRAIN_BUDGET, async {
        let mut panicked = false;
        while let Some(res) = set.join_next().await {
            if let Err(e) = res {
                tracing::error!(error = %e, "daemon.task-panicked");
                panicked = true;
            }
        }
        panicked
    })
    .await;

    match result {
        Ok(panicked) => {
            tracing::info!(forced = false, "daemon.shutdown-complete");
            panicked
        }
        Err(_elapsed) => {
            set.abort_all();
            tracing::warn!(forced = true, "daemon.shutdown-complete");
            false
        }
    }
}
