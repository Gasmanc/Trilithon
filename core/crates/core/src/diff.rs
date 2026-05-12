//! [`DiffEngine`] — structural diff between two [`DesiredState`] snapshots.
//!
//! The trait is pure: no I/O, no async.  [`DefaultDiffEngine`] implements the
//! flatten-and-compare algorithm described in Slice 8.1.
//!
//! # Ignored paths
//!
//! Slice 8.2 will supply an ignore-list.  Until then, [`is_ignored`] is a
//! no-op placeholder.

pub mod flatten;
pub mod ignore_list;
pub mod resolve;

use serde_json::Value;

use crate::canonical_json::canonicalise_value;
use crate::model::desired_state::DesiredState;
use crate::model::primitive::JsonPointer;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors returned by [`DiffEngine`] operations.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DiffError {
    /// The serialisation of a [`DesiredState`] failed unexpectedly.
    #[error("diff serialisation error: {detail}")]
    Serialisation {
        /// Human-readable detail.
        detail: String,
    },

    /// A structural conflict was detected between the before and after types.
    #[error("incompatible shape at {path}: cannot diff {before_kind} against {after_kind}")]
    IncompatibleShape {
        /// Pointer to the conflicting location.
        path: JsonPointer,
        /// Kind descriptor of the before value.
        before_kind: String,
        /// Kind descriptor of the after value.
        after_kind: String,
    },

    /// A secret value was found in plaintext inside the diff.
    #[error("redaction violated: plaintext secret remains at {path}")]
    RedactionViolated {
        /// Pointer to the un-redacted field.
        path: JsonPointer,
    },

    /// `apply_diff` attempted to create a value at a path whose parent does not
    /// exist and could not be inferred.
    #[error("missing parent path: {path}")]
    MissingParentPath {
        /// Pointer to the missing parent.
        path: JsonPointer,
    },
}

// ---------------------------------------------------------------------------
// Diff types
// ---------------------------------------------------------------------------

/// A single entry within a [`Diff`].
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DiffEntry {
    /// A leaf that exists in `after` but not `before`.
    Added {
        /// RFC 6901 pointer to the new value.
        path: JsonPointer,
        /// The value after the change.
        after: Value,
    },
    /// A leaf that exists in `before` but not `after`.
    Removed {
        /// RFC 6901 pointer to the removed value.
        path: JsonPointer,
        /// The value before the change.
        before: Value,
    },
    /// A leaf present in both but with a different value.
    Modified {
        /// RFC 6901 pointer to the changed value.
        path: JsonPointer,
        /// The value before the change.
        before: Value,
        /// The value after the change.
        after: Value,
    },
}

impl DiffEntry {
    /// Return the pointer associated with this entry.
    pub const fn path(&self) -> &JsonPointer {
        match self {
            Self::Added { path, .. } | Self::Removed { path, .. } | Self::Modified { path, .. } => {
                path
            }
        }
    }
}

/// Result of a structural diff between two [`DesiredState`] values.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Diff {
    /// All detected differences, sorted lexicographically by pointer.
    pub entries: Vec<DiffEntry>,
    /// Number of changes that were suppressed by the ignore list (Slice 8.2).
    pub ignored_count: u32,
}

impl Diff {
    /// Return `true` when the two states are byte-identical in canonical JSON.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Ignore-list (Slice 8.2)
// ---------------------------------------------------------------------------

/// Returns `true` when `path` should be excluded from the diff.
///
/// Delegates to the Caddy-managed paths list defined in [`ignore_list`].
#[inline]
fn is_ignored(path: &JsonPointer) -> bool {
    ignore_list::is_caddy_managed(path)
}

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Computes structural diffs between [`DesiredState`] pairs and applies them.
pub trait DiffEngine: Send + Sync + 'static {
    /// Compute the difference between `before` and `after`.
    ///
    /// Entries are sorted lexicographically by JSON pointer.  The
    /// [`Diff::ignored_count`] field reflects paths suppressed by the
    /// ignore list (Slice 8.2; currently always 0).
    ///
    /// # Errors
    ///
    /// Returns [`DiffError::Serialisation`] if either state cannot be
    /// serialised to canonical JSON.
    fn structural_diff(
        &self,
        before: &DesiredState,
        after: &DesiredState,
    ) -> Result<Diff, DiffError>;

    /// Apply `diff` to `state`, returning a new [`DesiredState`].
    ///
    /// This is the inverse of [`structural_diff`]: for any non-conflicting
    /// `(before, after)` pair,
    /// `apply_diff(before, structural_diff(before, after)) == after`.
    ///
    /// # Path-creation semantics
    ///
    /// When processing an [`DiffEntry::Added`] entry whose parent path does
    /// not yet exist, intermediate nodes are created automatically.  The
    /// kind of each intermediate node is inferred from the next path segment:
    /// - An all-digit segment implies a JSON array.
    /// - Any other segment implies a JSON object.
    ///
    /// If a required parent cannot be created (e.g. the segment type conflicts
    /// with an existing scalar), [`DiffError::MissingParentPath`] is returned.
    ///
    /// # Errors
    ///
    /// - [`DiffError::Serialisation`] — if either the input or output cannot
    ///   be round-tripped through canonical JSON / `DesiredState`.
    /// - [`DiffError::IncompatibleShape`] — a modification targets a path
    ///   whose current type is incompatible with the diff entry.
    /// - [`DiffError::MissingParentPath`] — a parent node is absent and
    ///   cannot be created.
    fn apply_diff(&self, state: &DesiredState, diff: &Diff) -> Result<DesiredState, DiffError>;
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

/// The standard [`DiffEngine`] that flattens both sides and computes set
/// differences.
pub struct DefaultDiffEngine;

impl DiffEngine for DefaultDiffEngine {
    fn structural_diff(
        &self,
        before: &DesiredState,
        after: &DesiredState,
    ) -> Result<Diff, DiffError> {
        // Serialise both sides to canonical JSON, then parse back to Value so
        // that the flattener sees a normalised representation.
        let val_before = state_to_value(before)?;
        let val_after = state_to_value(after)?;

        let flat_before = flatten::flatten(&val_before);
        let flat_after = flatten::flatten(&val_after);

        let mut entries: Vec<DiffEntry> = Vec::new();
        let mut ignored_count: u32 = 0;

        // Single pass over the union: walk `before` for Removed/Modified,
        // then walk `after` for Added.  Both maps are BTreeMaps so iteration
        // is already in key order — no extra sort is needed.
        for (path, bv) in &flat_before {
            if is_ignored(path) {
                ignored_count += 1;
                continue;
            }
            match flat_after.get(path) {
                None => entries.push(DiffEntry::Removed {
                    path: path.clone(),
                    before: bv.clone(),
                }),
                Some(av) if av != bv => entries.push(DiffEntry::Modified {
                    path: path.clone(),
                    before: bv.clone(),
                    after: av.clone(),
                }),
                Some(_) => {} // unchanged
            }
        }
        for (path, av) in &flat_after {
            if is_ignored(path) {
                // Only count once: skip paths already counted in the before loop.
                if !flat_before.contains_key(path) {
                    ignored_count += 1;
                }
                continue;
            }
            if !flat_before.contains_key(path) {
                entries.push(DiffEntry::Added {
                    path: path.clone(),
                    after: av.clone(),
                });
            }
        }

        // Merge the two already-sorted runs into a single sorted vec.
        entries.sort_unstable_by(|a, b| a.path().0.cmp(&b.path().0));

        Ok(Diff {
            entries,
            ignored_count,
        })
    }

    fn apply_diff(&self, state: &DesiredState, diff: &Diff) -> Result<DesiredState, DiffError> {
        let mut root = state_to_value(state)?;

        // Flatten the before-state so we can detect fully-removed subtrees.
        let flat_before = flatten::flatten(&root);

        let removed_leaves: std::collections::BTreeSet<&str> = diff
            .entries
            .iter()
            .filter_map(|e| {
                if let DiffEntry::Removed { path, .. } = e {
                    Some(path.as_str())
                } else {
                    None
                }
            })
            .collect();

        // Find subtree roots that are fully covered by `removed_leaves` — i.e.
        // every leaf under that root in `flat_before` is in `removed_leaves`.
        let subtrees = fully_removed_subtrees(&flat_before, &removed_leaves);

        // Apply subtree removals first (avoids leaving empty containers behind).
        for path in &subtrees {
            pointer_remove(&mut root, &JsonPointer((*path).to_owned()))?;
        }

        // Apply additions and modifications leaf-by-leaf.
        for entry in &diff.entries {
            match entry {
                DiffEntry::Added { path, after } => {
                    ensure_parent(&mut root, path)?;
                    pointer_set(&mut root, path, after.clone())?;
                }
                DiffEntry::Removed { path, .. } => {
                    // Only remove if not already covered by a subtree removal.
                    let covered = subtrees.iter().any(|root_str| {
                        let p = path.as_str();
                        p.starts_with(root_str)
                            && (p.len() == root_str.len()
                                || p.as_bytes().get(root_str.len()) == Some(&b'/'))
                    });
                    if !covered {
                        pointer_remove(&mut root, path)?;
                    }
                }
                DiffEntry::Modified { path, after, .. } => {
                    pointer_set(&mut root, path, after.clone())?;
                }
            }
        }

        // Reparse the mutated Value back into a DesiredState.
        serde_json::from_value(root).map_err(|e| DiffError::Serialisation {
            detail: e.to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a [`DesiredState`] to a canonical [`Value`].
///
/// Uses `serde_json::to_value` + in-place canonicalisation rather than
/// serialising to bytes and back, saving one allocation and two parse passes.
fn state_to_value(state: &DesiredState) -> Result<Value, DiffError> {
    serde_json::to_value(state)
        .map(canonicalise_value)
        .map_err(|e| DiffError::Serialisation {
            detail: e.to_string(),
        })
}

/// Return a human-readable kind name for a [`Value`].
const fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Parse a single RFC 6901 path segment back to its unescaped form.
fn unescape_segment(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

/// Walk `ptr` and ensure every intermediate node exists, creating containers
/// along the way using segment-kind inference.
fn ensure_parent(root: &mut Value, ptr: &JsonPointer) -> Result<(), DiffError> {
    let raw = ptr.as_str();
    if raw.is_empty() || raw == "/" {
        return Ok(());
    }
    // Strip leading slash; split into segments.
    let segments: Vec<&str> = raw.trim_start_matches('/').split('/').collect();
    // Only traverse up to the *parent* (all but the last segment).
    let parent_segments = &segments[..segments.len().saturating_sub(1)];

    let mut current = root;
    let mut reconstructed = String::new();

    for seg in parent_segments {
        reconstructed.push('/');
        reconstructed.push_str(seg);

        let unescaped = unescape_segment(seg);

        match current {
            Value::Object(map) => {
                if !map.contains_key(&unescaped) {
                    map.insert(unescaped.clone(), Value::Object(serde_json::Map::new()));
                }
                current = map
                    .get_mut(&unescaped)
                    .ok_or_else(|| DiffError::MissingParentPath {
                        path: JsonPointer(reconstructed.clone()),
                    })?;
            }
            Value::Array(arr) => {
                let idx: usize = unescaped
                    .parse()
                    .map_err(|_| DiffError::MissingParentPath {
                        path: JsonPointer(reconstructed.clone()),
                    })?;
                // Extend the array if needed.
                while arr.len() <= idx {
                    arr.push(Value::Object(serde_json::Map::new()));
                }
                current = arr
                    .get_mut(idx)
                    .ok_or_else(|| DiffError::MissingParentPath {
                        path: JsonPointer(reconstructed.clone()),
                    })?;
            }
            other => {
                return Err(DiffError::IncompatibleShape {
                    path: JsonPointer(reconstructed.clone()),
                    before_kind: kind_name(other).to_owned(),
                    after_kind: "object".to_owned(),
                });
            }
        }
    }

    Ok(())
}

/// Set the value at `ptr` inside `root`, returning an error if the path
/// resolves to a structural conflict.
fn pointer_set(root: &mut Value, ptr: &JsonPointer, new_val: Value) -> Result<(), DiffError> {
    let raw = ptr.as_str();
    if raw.is_empty() {
        *root = new_val;
        return Ok(());
    }

    let segments: Vec<&str> = raw.trim_start_matches('/').split('/').collect();
    let last = segments[segments.len() - 1];
    let last_unescaped = unescape_segment(last);

    // Navigate to parent.
    let mut current = root;
    let mut reconstructed = String::new();
    for seg in &segments[..segments.len() - 1] {
        reconstructed.push('/');
        reconstructed.push_str(seg);
        let unescaped = unescape_segment(seg);
        current = navigate_mut(current, &unescaped, &reconstructed)?;
    }

    // Set at leaf.
    let leaf_path = {
        let mut s = reconstructed.clone();
        s.push('/');
        s.push_str(last);
        s
    };
    match current {
        Value::Object(map) => {
            map.insert(last_unescaped, new_val);
            Ok(())
        }
        Value::Array(arr) => {
            let idx: usize = last_unescaped
                .parse()
                .map_err(|_| DiffError::MissingParentPath {
                    path: JsonPointer(leaf_path.clone()),
                })?;
            if idx < arr.len() {
                arr[idx] = new_val;
            } else {
                while arr.len() < idx {
                    arr.push(Value::Null);
                }
                arr.push(new_val);
            }
            Ok(())
        }
        other => Err(DiffError::IncompatibleShape {
            path: JsonPointer(leaf_path),
            before_kind: kind_name(other).to_owned(),
            after_kind: kind_name(&new_val).to_owned(),
        }),
    }
}

/// Remove the value at `ptr` from `root`.
fn pointer_remove(root: &mut Value, ptr: &JsonPointer) -> Result<(), DiffError> {
    let raw = ptr.as_str();
    if raw.is_empty() {
        return Ok(());
    }

    let segments: Vec<&str> = raw.trim_start_matches('/').split('/').collect();
    let last = segments[segments.len() - 1];
    let last_unescaped = unescape_segment(last);

    let mut current = root;
    let mut reconstructed = String::new();
    for seg in &segments[..segments.len() - 1] {
        reconstructed.push('/');
        reconstructed.push_str(seg);
        let unescaped = unescape_segment(seg);
        current = navigate_mut(current, &unescaped, &reconstructed)?;
    }

    match current {
        Value::Object(map) => {
            map.remove(&last_unescaped);
            Ok(())
        }
        Value::Array(arr) => {
            let idx: usize = last_unescaped
                .parse()
                .map_err(|_| DiffError::MissingParentPath { path: ptr.clone() })?;
            if idx < arr.len() {
                arr.remove(idx);
            }
            Ok(())
        }
        other => Err(DiffError::IncompatibleShape {
            path: ptr.clone(),
            before_kind: kind_name(other).to_owned(),
            after_kind: "none".to_owned(),
        }),
    }
}

/// Return the minimal set of JSON pointer strings whose subtrees are entirely
/// covered by `removed_leaves` within `flat_before`.
///
/// For each candidate prefix (each segment of each removed path), check
/// whether every leaf in `flat_before` that starts with that prefix is also
/// in `removed_leaves`.  The longest such prefix is the subtree root to
/// remove.  This avoids leaf-by-leaf removal which leaves empty containers
/// that break round-trip deserialization of enums.
fn fully_removed_subtrees<'a>(
    flat_before: &std::collections::BTreeMap<JsonPointer, Value>,
    removed_leaves: &std::collections::BTreeSet<&'a str>,
) -> Vec<&'a str> {
    // Collect candidate prefixes from removed paths, from shallowest to deepest.
    let mut candidates: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for path in removed_leaves {
        // Walk each prefix of each removed path.
        let mut depth = 0;
        for ch in path.chars() {
            depth += 1;
            if ch == '/' && depth > 1 {
                candidates.insert(&path[..depth - 1]);
            }
        }
        candidates.insert(path);
    }

    let mut chosen: Vec<&str> = Vec::new();
    // For each candidate (sorted shallowest first by length then lex),
    // check if all before-leaves under it are in removed_leaves.
    let mut sorted: Vec<&str> = candidates.into_iter().collect();
    sorted.sort_by_key(|s| s.len());

    for candidate in sorted {
        // Skip if already covered by a chosen ancestor.
        if chosen.iter().any(|c| {
            candidate.starts_with(c)
                && (candidate.len() == c.len() || candidate.as_bytes().get(c.len()) == Some(&b'/'))
        }) {
            continue;
        }
        // Check that every flat_before key under `candidate` is removed.
        let all_removed = flat_before.keys().all(|k| {
            let ks = k.as_str();
            if ks == candidate
                || (ks.starts_with(candidate) && ks.as_bytes().get(candidate.len()) == Some(&b'/'))
            {
                removed_leaves.contains(ks)
            } else {
                true // not under this candidate — irrelevant
            }
        });
        // Only choose if at least one before-leaf is actually under this candidate.
        let has_leaves = flat_before.keys().any(|k| {
            let ks = k.as_str();
            ks == candidate
                || (ks.starts_with(candidate) && ks.as_bytes().get(candidate.len()) == Some(&b'/'))
        });
        if all_removed && has_leaves {
            chosen.push(candidate);
        }
    }
    chosen
}

/// Navigate one level into `current` by `segment`, returning a mutable
/// reference to the child.
fn navigate_mut<'a>(
    current: &'a mut Value,
    segment: &str,
    reconstructed: &str,
) -> Result<&'a mut Value, DiffError> {
    match current {
        Value::Object(map) => map
            .get_mut(segment)
            .ok_or_else(|| DiffError::MissingParentPath {
                path: JsonPointer(reconstructed.to_owned()),
            }),
        Value::Array(arr) => {
            let idx: usize = segment.parse().map_err(|_| DiffError::MissingParentPath {
                path: JsonPointer(reconstructed.to_owned()),
            })?;
            arr.get_mut(idx)
                .ok_or_else(|| DiffError::MissingParentPath {
                    path: JsonPointer(reconstructed.to_owned()),
                })
        }
        other => Err(DiffError::IncompatibleShape {
            path: JsonPointer(reconstructed.to_owned()),
            before_kind: kind_name(other).to_owned(),
            after_kind: "object".to_owned(),
        }),
    }
}

// ---------------------------------------------------------------------------
// CaddyDiffEngine — legacy interface for adapter-layer equivalence checks
// ---------------------------------------------------------------------------

use crate::caddy::CaddyConfig;

/// Compares a rendered [`DesiredState`] against the live Caddy config.
///
/// This is the pre-Phase-8 interface, retained so that adapters can continue
/// to implement post-load equivalence checking (Step 5 of the apply algorithm)
/// without requiring an immediate migration to the full `DiffEngine`.
///
/// New code should prefer [`DiffEngine`].
pub trait CaddyDiffEngine: Send + Sync + 'static {
    /// Return the JSON pointer paths that differ between `desired` (rendered)
    /// and `observed` (from `GET /config/`).
    ///
    /// An empty `Vec` means the two are equivalent.
    ///
    /// # Errors
    ///
    /// Returns [`DiffError`] on serialisation failure.
    fn structural_diff(
        &self,
        desired: &DesiredState,
        observed: &CaddyConfig,
    ) -> Result<Vec<String>, DiffError>;
}

// ---------------------------------------------------------------------------
// ObjectKind — classifies a JSON pointer to a Caddy object category
// ---------------------------------------------------------------------------

/// High-level category of a Caddy configuration object.
///
/// Used by [`DiffCounts`] to bucket [`DiffEntry`] instances when producing a
/// [`DriftEvent`] summary.
#[derive(
    Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd, serde::Serialize, serde::Deserialize,
)]
pub enum ObjectKind {
    /// An HTTP route (`/apps/http/servers/*/routes/*`).
    Route,
    /// An upstream definition.
    Upstream,
    /// A TLS configuration entry (`/apps/tls/*`).
    Tls,
    /// An HTTP server (`/apps/http/servers/*`).
    Server,
    /// A policy attachment.
    Policy,
    /// Any path not matched by the above patterns.
    Other,
}

impl ObjectKind {
    /// Classify a [`JsonPointer`] into an [`ObjectKind`] using static prefix
    /// matching.
    ///
    /// Patterns are evaluated longest-first so that more specific matches
    /// shadow broader ones.
    #[must_use]
    pub fn classify(path: &JsonPointer) -> Self {
        let p = path.as_str();
        // Most-specific patterns first.
        if segment_match(p, &["/apps", "/http", "/servers", "*", "/routes", "*"]) {
            Self::Route
        } else if segment_match(p, &["/apps", "/tls", "*"]) {
            Self::Tls
        } else if segment_match(p, &["/apps", "/http", "/servers", "*"]) {
            Self::Server
        } else {
            Self::Other
        }
    }
}

/// Match `path` against a pattern where `"*"` matches any single segment.
///
/// The path must begin with the full pattern (may have additional trailing
/// segments) for the match to succeed.
fn segment_match(path: &str, pattern: &[&str]) -> bool {
    // Strip leading slash; split into segments.
    let mut segs = path.trim_start_matches('/').split('/');
    for &pat in pattern {
        match segs.next() {
            None => return false,
            Some(seg) => {
                if pat != "*" && pat.trim_start_matches('/') != seg {
                    return false;
                }
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// DiffCounts — per-kind summary of diff entries
// ---------------------------------------------------------------------------

/// Count of added, removed, and modified entries for a single [`ObjectKind`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DiffCounts {
    /// Number of entries added.
    pub added: u32,
    /// Number of entries removed.
    pub removed: u32,
    /// Number of entries modified.
    pub modified: u32,
}

impl DiffCounts {
    /// Accumulate one [`DiffEntry`] into the appropriate counter.
    const fn tally(&mut self, entry: &DiffEntry) {
        match entry {
            DiffEntry::Added { .. } => self.added += 1,
            DiffEntry::Removed { .. } => self.removed += 1,
            DiffEntry::Modified { .. } => self.modified += 1,
        }
    }
}

/// Build a [`BTreeMap`] from [`ObjectKind`] to [`DiffCounts`] by classifying
/// every entry in `diff`.
#[must_use]
pub fn summarise_diff(diff: &Diff) -> std::collections::BTreeMap<ObjectKind, DiffCounts> {
    let mut map: std::collections::BTreeMap<ObjectKind, DiffCounts> =
        std::collections::BTreeMap::new();
    for entry in &diff.entries {
        let kind = ObjectKind::classify(entry.path());
        map.entry(kind).or_default().tally(entry);
    }
    map
}

// ---------------------------------------------------------------------------
// DriftEvent — record stored by Storage::record_drift_event
// ---------------------------------------------------------------------------

use crate::storage::types::SnapshotId;

/// A full record of a single detected drift event.
///
/// Constructed by the detector (Slice 8.5) after running the diff engine and
/// redactor.  Persisted via `Storage::record_drift_event`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DriftEvent {
    /// The snapshot that was the expected ("desired") state at detection time.
    pub before_snapshot_id: SnapshotId,
    /// SHA-256 hash (lowercase hex) of the live running-state JSON.
    pub running_state_hash: String,
    /// Per-kind breakdown of the diff.
    pub diff_summary: std::collections::BTreeMap<ObjectKind, DiffCounts>,
    /// Unix timestamp (seconds, UTC) when the drift was detected.
    pub detected_at: i64,
    /// Correlation token linking this event to the triggering detection cycle.
    pub correlation_id: ulid::Ulid,
    /// Canonical JSON (keys sorted lexicographically) of the redacted diff.
    pub redacted_diff_json: String,
    /// Number of values that were redacted from the diff.
    pub redaction_sites: u32,
}

// ---------------------------------------------------------------------------
// NoOpDiffEngine — implements BOTH traits
// ---------------------------------------------------------------------------

/// A no-op that always reports no differences.
///
/// Implements both [`DiffEngine`] (Phase 8) and [`CaddyDiffEngine`] (legacy).
pub struct NoOpDiffEngine;

impl DiffEngine for NoOpDiffEngine {
    fn structural_diff(
        &self,
        _before: &DesiredState,
        _after: &DesiredState,
    ) -> Result<Diff, DiffError> {
        Ok(Diff {
            entries: Vec::new(),
            ignored_count: 0,
        })
    }

    fn apply_diff(&self, state: &DesiredState, _diff: &Diff) -> Result<DesiredState, DiffError> {
        Ok(state.clone())
    }
}

impl CaddyDiffEngine for NoOpDiffEngine {
    fn structural_diff(
        &self,
        _desired: &DesiredState,
        _observed: &CaddyConfig,
    ) -> Result<Vec<String>, DiffError> {
        Ok(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::disallowed_methods,
    missing_docs
)]
// reason: test-only code; panics are the correct failure mode in tests
mod tests {
    use super::*;
    use crate::canonical_json::to_canonical_bytes;
    use crate::model::{
        desired_state::DesiredState,
        header::HeaderRules,
        identifiers::{RouteId, UpstreamId},
        matcher::MatcherSet,
        route::{HostPattern, Route},
        upstream::{Upstream, UpstreamDestination, UpstreamProbe},
    };

    fn engine() -> DefaultDiffEngine {
        DefaultDiffEngine
    }

    fn empty() -> DesiredState {
        DesiredState::empty()
    }

    fn make_route(id: &str, host: &str) -> Route {
        Route {
            id: RouteId(id.to_owned()),
            hostnames: vec![HostPattern::Exact(host.to_owned())],
            upstreams: vec![],
            matchers: MatcherSet::default(),
            headers: HeaderRules::default(),
            redirects: None,
            policy_attachment: None,
            enabled: true,
            created_at: 0,
            updated_at: 0,
        }
    }

    fn make_upstream(id: &str, port: u16) -> Upstream {
        Upstream {
            id: UpstreamId(id.to_owned()),
            destination: UpstreamDestination::TcpAddr {
                host: "127.0.0.1".to_owned(),
                port,
            },
            probe: UpstreamProbe::Disabled,
            weight: 1,
            max_request_bytes: None,
        }
    }

    // -----------------------------------------------------------------------
    // Spec tests
    // -----------------------------------------------------------------------

    #[test]
    fn adds_detected() {
        let before = empty();
        let mut after = empty();
        after
            .upstreams
            .insert(UpstreamId("U1".to_owned()), make_upstream("U1", 9000));

        let diff = engine().structural_diff(&before, &after).expect("ok");

        assert!(!diff.is_empty(), "should have additions");
        assert!(
            diff.entries
                .iter()
                .any(|e| matches!(e, DiffEntry::Added { .. })),
            "must have at least one Added entry"
        );
    }

    #[test]
    fn removes_detected() {
        let mut before = empty();
        before
            .upstreams
            .insert(UpstreamId("U1".to_owned()), make_upstream("U1", 9000));
        let after = empty();

        let diff = engine().structural_diff(&before, &after).expect("ok");

        assert!(!diff.is_empty());
        assert!(
            diff.entries
                .iter()
                .any(|e| matches!(e, DiffEntry::Removed { .. }))
        );
    }

    #[test]
    fn modifies_detected() {
        let mut before = empty();
        before
            .routes
            .insert(RouteId("R1".to_owned()), make_route("R1", "a.example.com"));
        let mut after = empty();
        after
            .routes
            .insert(RouteId("R1".to_owned()), make_route("R1", "b.example.com"));

        let diff = engine().structural_diff(&before, &after).expect("ok");

        assert!(!diff.is_empty());
        assert!(
            diff.entries
                .iter()
                .any(|e| matches!(e, DiffEntry::Modified { .. }))
        );
    }

    #[test]
    fn unchanged_returns_empty_diff() {
        let state = {
            let mut s = empty();
            s.routes
                .insert(RouteId("R1".to_owned()), make_route("R1", "x.example.com"));
            s
        };

        let diff = engine().structural_diff(&state, &state).expect("ok");
        assert!(
            diff.is_empty(),
            "identical states must produce an empty diff"
        );
    }

    #[test]
    fn array_index_pointer_format() {
        // Build two states that differ at a nested array path.
        let mut before = empty();
        before.routes.insert(
            RouteId("R1".to_owned()),
            make_route("R1", "before.example.com"),
        );
        let mut after = empty();
        after.routes.insert(
            RouteId("R1".to_owned()),
            make_route("R1", "after.example.com"),
        );

        let diff = engine().structural_diff(&before, &after).expect("ok");

        // The diff must include a pointer that contains a segment from the
        // routes object and at least one nested segment.
        let has_routes_path = diff
            .entries
            .iter()
            .any(|e| e.path().as_str().contains("routes") && e.path().as_str().contains("R1"));
        assert!(has_routes_path, "expected a diff entry under /routes/R1/…");
    }

    #[test]
    fn deterministic_ordering() {
        let before = empty();
        let mut after = empty();
        after
            .routes
            .insert(RouteId("Z".to_owned()), make_route("Z", "z.example.com"));
        after
            .routes
            .insert(RouteId("A".to_owned()), make_route("A", "a.example.com"));
        after
            .upstreams
            .insert(UpstreamId("U1".to_owned()), make_upstream("U1", 9001));

        let diff = engine().structural_diff(&before, &after).expect("ok");

        let pointers: Vec<&str> = diff.entries.iter().map(|e| e.path().as_str()).collect();
        let mut sorted = pointers.clone();
        sorted.sort_unstable();
        assert_eq!(pointers, sorted, "diff entries must be sorted by pointer");
    }

    #[test]
    fn apply_diff_inverse_round_trip() {
        let mut state_a = empty();
        state_a
            .routes
            .insert(RouteId("R1".to_owned()), make_route("R1", "a.example.com"));
        state_a
            .upstreams
            .insert(UpstreamId("U1".to_owned()), make_upstream("U1", 8080));

        let mut state_b = empty();
        state_b
            .routes
            .insert(RouteId("R1".to_owned()), make_route("R1", "b.example.com"));
        state_b
            .upstreams
            .insert(UpstreamId("U2".to_owned()), make_upstream("U2", 9090));

        let eng = engine();
        let diff = eng.structural_diff(&state_a, &state_b).expect("diff");
        let reconstructed = eng.apply_diff(&state_a, &diff).expect("apply");

        // Compare via canonical JSON bytes to avoid any serde field-order issues.
        let b_bytes = to_canonical_bytes(&state_b).expect("state_b bytes");
        let r_bytes = to_canonical_bytes(&reconstructed).expect("reconstructed bytes");
        assert_eq!(
            b_bytes, r_bytes,
            "apply_diff must be the inverse of structural_diff"
        );
    }

    // -----------------------------------------------------------------------
    // Slice 8.3: DriftEvent serde round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn drift_event_serde_round_trip() {
        use std::collections::BTreeMap;
        use ulid::Ulid;

        let mut summary = BTreeMap::new();
        summary.insert(
            ObjectKind::Route,
            DiffCounts {
                added: 1,
                removed: 0,
                modified: 2,
            },
        );
        summary.insert(
            ObjectKind::Server,
            DiffCounts {
                added: 0,
                removed: 1,
                modified: 0,
            },
        );

        let event = DriftEvent {
            before_snapshot_id: crate::storage::types::SnapshotId("snap-abc".to_owned()),
            running_state_hash: "a".repeat(64),
            diff_summary: summary,
            detected_at: 1_700_000_000,
            correlation_id: Ulid::from_string("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap(),
            redacted_diff_json: r#"{"redacted":true}"#.to_owned(),
            redaction_sites: 3,
        };

        let json = serde_json::to_string(&event).unwrap();
        let restored: DriftEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    // -----------------------------------------------------------------------
    // Slice 8.3: DiffCounts classifier
    // -----------------------------------------------------------------------

    #[test]
    fn diff_counts_classifier() {
        let cases: &[(&str, ObjectKind)] = &[
            (
                "/apps/http/servers/srv0/routes/r1/hostnames/0",
                ObjectKind::Route,
            ),
            ("/apps/http/servers/srv0/routes/r2", ObjectKind::Route),
            ("/apps/tls/certificates/0", ObjectKind::Tls),
            ("/apps/http/servers/srv0/listen/0", ObjectKind::Server),
            ("/apps/http/servers/srv0", ObjectKind::Server),
            ("/logging/logs/default/level", ObjectKind::Other),
            ("/admin/listen", ObjectKind::Other),
        ];
        for (path, expected) in cases {
            let ptr = JsonPointer((*path).to_owned());
            let got = ObjectKind::classify(&ptr);
            assert_eq!(&got, expected, "path: {path}");
        }
    }

    // -----------------------------------------------------------------------
    // Backward compat: no_op_always_empty is preserved
    // -----------------------------------------------------------------------

    #[test]
    fn no_op_always_empty() {
        let engine = NoOpDiffEngine;
        let desired = DesiredState::empty();
        // Fully qualify to disambiguate from CaddyDiffEngine::structural_diff.
        let diff = DiffEngine::structural_diff(&engine, &desired, &desired).expect("ok");
        assert!(
            diff.is_empty(),
            "NoOpDiffEngine must always return an empty diff"
        );
    }
}
