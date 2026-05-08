//! Integration test: cursor-based descending pagination.
//!
//! Inserts 100 rows with deterministic, monotonically increasing ULID ids.
//! Paginates through in batches of 25 using `cursor_before`.
//!
//! Asserts:
//! - No row is returned more than once.
//! - No row is skipped (all 100 are visited).
//! - Each batch is in descending id order.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    clippy::cast_possible_wrap
)]
// reason: test-only code; panics are the correct failure mode in tests; casts
// are bounded by test-controlled values that cannot overflow i64

use std::collections::HashSet;

use tempfile::TempDir;
use trilithon_adapters::{migrate::apply_migrations, sqlite_storage::SqliteStorage};
use trilithon_core::storage::{
    helpers::audit_prev_hash_seed,
    trait_def::Storage,
    types::{ActorKind, AuditEventRow, AuditOutcome, AuditRowId, AuditSelector},
};
use ulid::Ulid;

async fn open(dir: &TempDir) -> SqliteStorage {
    let store = SqliteStorage::open(dir.path())
        .await
        .expect("SqliteStorage::open should succeed");
    apply_migrations(store.pool())
        .await
        .expect("apply_migrations should succeed");
    store
}

fn make_row(i: u64) -> AuditEventRow {
    // Use a fixed high-bit timestamp so ULIDs sort deterministically.
    AuditEventRow {
        id: AuditRowId(Ulid::from_parts(1_700_000_000_000 + i, 0).to_string()),
        prev_hash: audit_prev_hash_seed().to_owned(),
        caddy_instance_id: "local".to_owned(),
        correlation_id: Ulid::new().to_string(),
        occurred_at: 1_700_000_000 + i as i64,
        occurred_at_ms: (1_700_000_000_000 + i) as i64,
        actor_kind: ActorKind::System,
        actor_id: "test-actor".to_owned(),
        kind: "config.applied".to_owned(),
        target_kind: None,
        target_id: None,
        snapshot_id: None,
        redacted_diff_json: None,
        redaction_sites: 0,
        outcome: AuditOutcome::Ok,
        error_kind: None,
        notes: None,
    }
}

const TOTAL_ROWS: usize = 100;
const PAGE_SIZE: u32 = 25;

#[tokio::test]
async fn cursor_pagination_visits_every_row_exactly_once() {
    let dir = TempDir::new().unwrap();
    let store = open(&dir).await;

    // Insert rows in ascending order.
    for i in 0..TOTAL_ROWS as u64 {
        store
            .record_audit_event(make_row(i))
            .await
            .expect("insert must succeed");
    }

    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut cursor: Option<AuditRowId> = None;
    let mut page_count = 0_usize;

    loop {
        let selector = AuditSelector {
            cursor_before: cursor.clone(),
            ..Default::default()
        };

        let batch = store
            .tail_audit_log(selector, PAGE_SIZE)
            .await
            .expect("tail_audit_log must succeed");

        if batch.is_empty() {
            break;
        }

        // Assert descending order within each batch.
        for window in batch.windows(2) {
            assert!(
                window[0].id.0 > window[1].id.0,
                "batch must be in descending id order: {:?} >= {:?}",
                window[0].id.0,
                window[1].id.0
            );
        }

        // Assert no duplicates.
        for row in &batch {
            let inserted = seen_ids.insert(row.id.0.clone());
            assert!(inserted, "row {:?} was returned more than once", row.id.0);
        }

        // Advance cursor to the smallest id in this batch (last element since DESC).
        cursor = Some(batch.last().unwrap().id.clone());
        page_count += 1;

        // Safety valve: avoid infinite loop in case of a bug.
        assert!(
            page_count <= TOTAL_ROWS,
            "pagination loop exceeded {TOTAL_ROWS} pages — likely infinite loop"
        );
    }

    assert_eq!(
        seen_ids.len(),
        TOTAL_ROWS,
        "expected {TOTAL_ROWS} unique rows across all pages, got {}",
        seen_ids.len()
    );

    // 100 rows / 25 per page = exactly 4 pages.
    assert_eq!(page_count, 4, "expected 4 pages of 25, got {page_count}");
}
