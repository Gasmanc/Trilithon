---
track: knowledge
problem_type: best-practice
root_cause: insufficient-validation
resolution_type: validation-added
severity: high
title: "Re-derive content-addressed id from payload at the storage write path rather than trusting caller"
slug: storage-rederive-content-address-on-write
date: 2026-05-05
phase_id: "5"
generalizable: true
one_sentence_lesson: "At the storage write boundary, always re-derive the content-addressed identifier from the payload rather than trusting the caller-supplied value — a mismatch silently accepts a corrupt or spoofed snapshot."
tags: [rust, storage, content-addressing, trust-boundary, validation]
---

## Context

`Snapshot` carries both `snapshot_id` (a SHA-256 hex of `desired_state_json`) and the JSON payload. The `insert_snapshot` implementation initially trusted the caller-supplied `snapshot_id` without verifying it matched the payload.

## What Happened

Any caller that constructed a `Snapshot` with a mismatched `snapshot_id` — whether from a bug in the construction code or a serialisation error — would silently persist a corrupt record. The content-addressing guarantee (id == hash(body)) would be broken in the database with no error surfaced.

The fix adds `validate_snapshot_invariants` at the top of the write path, which recomputes the SHA-256 of `desired_state_json` and returns `StorageError::SnapshotHashCollision` if it does not match `snapshot_id`. This also enforces the intent length bound at the same point.

```rust
fn validate_snapshot_invariants(snapshot: &Snapshot) -> Result<(), StorageError> {
    if !Snapshot::validate_intent(&snapshot.intent) {
        return Err(StorageError::IntentTooLong { ... });
    }
    let expected_id = content_address(snapshot.desired_state_json.as_bytes());
    if expected_id != snapshot.snapshot_id.0 {
        return Err(StorageError::SnapshotHashCollision { ... });
    }
    Ok(())
}
```

## Lesson

> At the storage write boundary, always re-derive the content-addressed identifier from the payload rather than trusting the caller-supplied value — a mismatch silently accepts a corrupt or spoofed snapshot.

## Applies When

- Any content-addressed store where the identifier is derived from the payload (SHA-256, BLAKE3, etc.)
- Storage traits with multiple implementations — validate in the adapter, not only in the caller
- Write paths that accept structs carrying both the payload and its precomputed id

## Does Not Apply When

- The identifier is an opaque primary key unrelated to payload content (ULID, UUID, autoincrement)
- The storage layer is an internal cache where the caller is the sole writer and validation happens upstream
