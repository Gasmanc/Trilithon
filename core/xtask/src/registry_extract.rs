//! xtask: registry-extract
//!
//! Foundation 1/B — generates `docs/architecture/contracts.md` from source.
//!
//! Reads `docs/architecture/contract-roots.toml` and walks every Rust file in the
//! workspace looking for items annotated with `// contract:` on their definition line.
//!
//! THIS FILE IS A TEMPLATE. The full implementation requires `syn` for Rust
//! parsing, `toml` for the roots config, and `walkdir` for source traversal.
//! Production code MUST handle parse errors gracefully — partial extraction
//! is acceptable but the coverage report MUST list every skipped item.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const ROOTS_PATH: &str = "docs/architecture/contract-roots.toml";
const OUT_MD: &str = "docs/architecture/contracts.md";
const OUT_COVERAGE: &str = "docs/architecture/contracts-coverage.json";
const CONTRACT_MARKER: &str = "// contract:";

#[derive(Default)]
struct Coverage {
    parsed: usize,
    skipped_parse_error: Vec<String>,
}

pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let write = args.iter().any(|a| a == "--write");

    let mut contracts: BTreeMap<String, ContractEntry> = BTreeMap::new();
    let mut coverage = Coverage::default();

    // Scan for `// contract:` markers across all Rust sources.
    scan_for_markers(Path::new("core/crates"), &mut contracts, &mut coverage);

    let partial = !coverage.skipped_parse_error.is_empty();
    let md = render_markdown(&contracts, partial, &coverage);
    let cov_json = render_coverage_json(&coverage);

    if write {
        fs::write(OUT_MD, &md).unwrap();
        fs::write(OUT_COVERAGE, &cov_json).unwrap();
        println!(
            "wrote {} ({} contracts, partial: {})",
            OUT_MD,
            contracts.len(),
            partial
        );
    } else {
        println!("{}", md);
    }
}

struct ContractEntry {
    crate_name: String,
    source_file: PathBuf,
    source_line: usize,
    annotation: String,
}

fn scan_for_markers(
    root: &Path,
    contracts: &mut BTreeMap<String, ContractEntry>,
    coverage: &mut Coverage,
) {
    if !root.exists() {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_for_markers(&path, contracts, coverage);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            coverage.parsed += 1;
            let Ok(src) = fs::read_to_string(&path) else {
                coverage
                    .skipped_parse_error
                    .push(path.display().to_string());
                continue;
            };
            for (i, line) in src.lines().enumerate() {
                if let Some(annotation) = line
                    .find(CONTRACT_MARKER)
                    .map(|idx| line[idx + CONTRACT_MARKER.len()..].trim())
                {
                    // Next non-blank line is the item definition.
                    let item_name = src
                        .lines()
                        .skip(i + 1)
                        .find(|l| !l.trim().is_empty())
                        .and_then(extract_item_name)
                        .unwrap_or("unknown");
                    let crate_name = path
                        .components()
                        .find(|c| matches!(c, std::path::Component::Normal(_)))
                        .and_then(|c| c.as_os_str().to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let key = format!("{}::{}", crate_name, item_name);
                    contracts.insert(
                        key,
                        ContractEntry {
                            crate_name,
                            source_file: path.clone(),
                            source_line: i + 1,
                            annotation: annotation.to_string(),
                        },
                    );
                }
            }
        }
    }
}

fn extract_item_name(line: &str) -> Option<&str> {
    for tok in [
        "pub fn ",
        "pub async fn ",
        "pub trait ",
        "pub struct ",
        "pub enum ",
        "pub type ",
    ] {
        if let Some(i) = line.find(tok) {
            let rest = &line[i + tok.len()..];
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            return Some(&rest[..end]);
        }
    }
    None
}

fn render_markdown(
    contracts: &BTreeMap<String, ContractEntry>,
    partial: bool,
    coverage: &Coverage,
) -> String {
    let mut s = String::new();
    s.push_str("---\n");
    s.push_str(&format!("partial: {}\n", partial));
    s.push_str(&format!("contract_count: {}\n", contracts.len()));
    s.push_str(&format!("parsed_files: {}\n", coverage.parsed));
    s.push_str(&format!(
        "skipped_files: {}\n",
        coverage.skipped_parse_error.len()
    ));
    s.push_str("schema_version: 1\n");
    s.push_str("---\n\n");
    s.push_str("# Contract Registry\n\n");
    s.push_str("**Generated by `cargo xtask registry-extract` — do not hand-edit.**\n");
    s.push_str("Invariants live in `contracts-invariants.md` (human-curated).\n\n");
    if partial {
        s.push_str("> ⚠ **Partial extraction.** Some source files could not be read.\n");
        s.push_str("> See `contracts-coverage.json` for details.\n\n");
    }
    if contracts.is_empty() {
        s.push_str(
            "> No contracts declared yet. Annotate items with `// contract: <description>`\n",
        );
        s.push_str("> or add entries to `docs/architecture/contract-roots.toml`.\n\n");
        s.push_str("_(empty)_\n");
    } else {
        for (key, entry) in contracts {
            s.push_str(&format!("## `{}`\n\n", key));
            if !entry.annotation.is_empty() {
                s.push_str(&format!("> {}\n\n", entry.annotation));
            }
            s.push_str(&format!(
                "Source: `{}:{}`\n\n",
                entry.source_file.display(),
                entry.source_line
            ));
        }
    }
    s
}

fn render_coverage_json(coverage: &Coverage) -> String {
    format!(
        "{{\"parsed\":{},\"skipped\":{}}}\n",
        coverage.parsed,
        coverage.skipped_parse_error.len()
    )
}
