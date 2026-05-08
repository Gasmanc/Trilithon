//! xtask: audit-finding-schema — validate F0 frontmatter on all findings.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;

const SCAN_DIRS: &[&str] = &[
    "docs/End_of_Phase_Reviews/Findings",
    "docs/End_of_Phase_Reviews/Unfixed",
    "docs/End_of_Phase_Reviews/Fixed",
    "docs/In_Flight_Reviews/Findings",
    "docs/In_Flight_Reviews/Unfixed",
    "docs/In_Flight_Reviews/Fixed",
];

pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let report_only = args.iter().any(|a| a == "--report");

    let mut total = 0usize;
    let mut invalid: Vec<(PathBuf, String)> = Vec::new();

    for dir in SCAN_DIRS {
        let p = Path::new(dir);
        if !p.exists() {
            continue;
        }
        for f in walk_md(p) {
            total += 1;
            if let Err(reason) = validate_frontmatter(&f) {
                invalid.push((f, reason));
            }
        }
    }

    println!("audit-finding-schema:");
    println!("  total findings:    {}", total);
    println!("  invalid:           {}", invalid.len());

    if !invalid.is_empty() {
        for (f, r) in &invalid {
            eprintln!("  ✗ {} — {}", f.display(), r);
        }
        if !report_only {
            exit(1);
        }
    }
}

fn walk_md(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                out.extend(walk_md(&p));
            } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if stem != "README" {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn validate_frontmatter(path: &Path) -> Result<(), String> {
    let body = fs::read_to_string(path).map_err(|e| e.to_string())?;
    if !body.starts_with("---\n") {
        return Err("missing YAML frontmatter".to_string());
    }
    let after = &body[4..];
    let end = after
        .find("\n---\n")
        .ok_or_else(|| "frontmatter not terminated".to_string())?;
    let fm = &after[..end];

    for field in &[
        "id:",
        "category:",
        "kind:",
        "finding_kind:",
        "status:",
        "last_verified_at:",
    ] {
        if !fm.contains(field) {
            return Err(format!(
                "missing required field: {}",
                field.trim_end_matches(':')
            ));
        }
    }
    Ok(())
}
