//! Bootstrap does not emit the plaintext password in any tracing event.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods
)]
// reason: test-only code; panics are the correct failure mode in tests

use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

use tempfile::TempDir;
use tracing::Subscriber;
use tracing_subscriber::{Layer, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use trilithon_adapters::{
    AuditWriter, Sha256AuditHasher,
    auth::{bootstrap::bootstrap_if_empty, users::SqliteUserStore},
    migrate::apply_migrations,
    rng::RandomBytes,
    sqlite_storage::SqliteStorage,
};
use trilithon_core::{clock::SystemClock, schema::SchemaRegistry, storage::trait_def::Storage};

// ── Deterministic RNG: always returns 0x42 bytes ─────────────────────────────

struct FixedRng;

impl RandomBytes for FixedRng {
    fn fill_bytes(&self, buf: &mut [u8]) {
        buf.fill(0x42);
    }
}

// ── Log-capture layer ─────────────────────────────────────────────────────────

/// Collects all formatted tracing events into a shared buffer.
#[derive(Clone)]
struct LogCapture {
    lines: Arc<Mutex<Vec<String>>>,
}

impl LogCapture {
    fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn collected(&self) -> Vec<String> {
        self.lines.lock().unwrap().clone()
    }
}

impl<S: Subscriber> Layer<S> for LogCapture {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut visitor = StringVisitor(String::new());
        event.record(&mut visitor);
        let msg = visitor.0;
        self.lines.lock().unwrap().push(msg);
    }
}

struct StringVisitor(String);

impl tracing::field::Visit for StringVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let _ = write!(&mut self.0, " {}={:?}", field.name(), value);
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        let _ = write!(&mut self.0, " {}={}", field.name(), value);
    }
}

#[tokio::test]
async fn bootstrap_password_not_in_logs() {
    let dir = TempDir::new().unwrap();
    let storage = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open");
    apply_migrations(storage.pool()).await.expect("migrations");
    let pool = storage.pool().clone();
    let user_store = SqliteUserStore::new(pool);

    let storage_arc: Arc<dyn Storage> = Arc::new(storage);
    let audit = AuditWriter::new_with_arcs(
        storage_arc,
        Arc::new(SystemClock),
        Arc::new(SchemaRegistry::with_tier1_secrets()),
        Arc::new(Sha256AuditHasher),
    );

    let capture = LogCapture::new();
    let capture_clone = capture.clone();

    // Install a scoped subscriber that captures events during bootstrap.
    let subscriber = tracing_subscriber::registry().with(capture_clone);
    let _guard = subscriber.set_default();

    let outcome = bootstrap_if_empty(&user_store, &FixedRng, dir.path(), &audit)
        .await
        .expect("bootstrap_if_empty must succeed")
        .expect("must return Some on fresh store");

    // Derive the expected password from the fixed RNG (same bytes as FixedRng produces).
    // The password is encode_password([0x42; 18]) — we check none of the collected
    // log lines contain it.  We verify by reading the file instead.
    let creds = std::fs::read_to_string(&outcome.credentials_path).expect("read creds file");
    let password_line = creds
        .lines()
        .find(|l| l.starts_with("password:"))
        .expect("password line in credentials file");
    let password = password_line
        .strip_prefix("password: ")
        .expect("password value");

    let all_logs = capture.collected().join("\n");
    assert!(
        !all_logs.contains(password),
        "plaintext password must not appear in any log line.\nPassword: {password}\nLogs:\n{all_logs}"
    );
}
