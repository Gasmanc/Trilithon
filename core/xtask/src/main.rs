//! xtask — cross-phase coherence tooling dispatcher.
//!
//! Run with: `cargo xtask <subcommand>`
//! (from the `core/` workspace root, or via `just migrate-findings` etc.)
//!
//! All subcommands run with their working directory set to the project root
//! (the parent of this workspace), so relative paths like `docs/` and
//! `.claude/` resolve correctly regardless of where cargo was invoked from.

// Tooling stubs — allow dead code and relaxed style until bodies are filled in.
#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    clippy::all,
    clippy::pedantic,
    clippy::nursery
)]

mod audit_duplicates;
mod audit_finding_schema;
mod call_graph_summary;
mod cross_cutting_matrix;
mod identifier_frequency;
mod invariant_check;
mod migrate_findings;
mod normalize_finding;
mod registry_check;
mod registry_extract;
mod registry_verify;
mod revalidate_findings;
mod set_merge_review_baseline;

use std::path::Path;

fn main() {
    // Navigate to the project root (core/xtask/../../) before dispatching,
    // so all relative paths in subcommands (docs/, .claude/, etc.) work.
    let project_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has no parent (core/)")
        .parent()
        .expect("core/ has no parent (project root)");
    std::env::set_current_dir(project_root).expect("failed to chdir to project root");

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("migrate-findings") => migrate_findings::run(),
        Some("registry-extract") => registry_extract::run(),
        Some("registry-verify") => registry_verify::run(),
        Some("registry-check") => registry_check::run(),
        Some("invariant-check") => invariant_check::run(),
        Some("set-merge-review-baseline") => set_merge_review_baseline::run(),
        Some("audit-finding-schema") => audit_finding_schema::run(),
        Some("audit-duplicates") => audit_duplicates::run(),
        Some("revalidate-findings") => revalidate_findings::run(),
        Some("normalize-finding") => normalize_finding::run(),
        Some("identifier-frequency") => identifier_frequency::run(),
        Some("call-graph-summary") => call_graph_summary::run(),
        Some("cross-cutting-matrix") => cross_cutting_matrix::run(),
        Some(other) => {
            eprintln!("unknown xtask: {other}");
            eprintln!();
            eprintln!("available subcommands:");
            eprintln!("  migrate-findings          one-time F0 schema migration");
            eprintln!("  registry-extract          generate docs/architecture/contracts.md");
            eprintln!("  registry-verify           verify contracts.md is up to date");
            eprintln!("  registry-check            gate on contract drift");
            eprintln!("  invariant-check           verify cited symbols exist in contracts.md");
            eprintln!("  set-merge-review-baseline set adoption baseline SHA");
            eprintln!("  audit-finding-schema      validate F0 frontmatter on all findings");
            eprintln!("  audit-duplicates          structural duplicate detector");
            eprintln!("  revalidate-findings       refresh open findings against main HEAD");
            eprintln!("  normalize-finding         convert raw reviewer output to F0 files");
            eprintln!("  identifier-frequency      terminology drift heuristic (advisory)");
            eprintln!("  call-graph-summary        best-effort call graph");
            eprintln!("  cross-cutting-matrix      per-area concern presence/absence matrix");
            std::process::exit(2);
        }
        None => {
            eprintln!("usage: cargo xtask <subcommand>");
            eprintln!("run `cargo xtask help` (or any unknown subcommand) for list");
            std::process::exit(2);
        }
    }
}
