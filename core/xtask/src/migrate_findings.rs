//! xtask: migrate-findings
//!
//! One-time migration of pre-Foundation-0 finding artefacts to the structured
//! schema. Idempotent — re-running on already-migrated files is a no-op.
//!
//! Run as: `cargo xtask migrate-findings [--dry-run] [--baseline-sha <sha>]`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// Finding directories relative to the project root.
const FINDING_DIRS: &[&str] = &[
    "docs/End_of_Phase_Reviews/Findings",
    "docs/End_of_Phase_Reviews/Unfixed",
    "docs/End_of_Phase_Reviews/Fixed",
    "docs/In_Flight_Reviews/Findings",
    "docs/In_Flight_Reviews/Unfixed",
    "docs/In_Flight_Reviews/Fixed",
    "docs/In_Flight_Reviews/In_Flight_Review_Findings",
    "docs/In_Flight_Reviews/In_Flight_Review_Unfixed",
    "docs/In_Flight_Reviews/In_Flight_Review_Fixed",
];

pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let baseline_sha = args
        .windows(2)
        .find(|w| w[0] == "--baseline-sha")
        .map(|w| w[1].clone())
        .unwrap_or_else(|| current_main_sha().unwrap_or_else(|_| "unknown".to_string()));

    let mut migrated = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    for dir in FINDING_DIRS {
        let p = Path::new(dir);
        if !p.exists() {
            continue;
        }
        for entry in walk_md(p).unwrap_or_default() {
            match migrate_file(&entry, &baseline_sha, dry_run) {
                Ok(true) => migrated += 1,
                Ok(false) => skipped += 1,
                Err(e) => errors.push(format!("{}: {}", entry.display(), e)),
            }
        }
    }

    println!("migrate-findings:");
    println!("  migrated: {}", migrated);
    println!("  skipped (already F0): {}", skipped);
    println!("  errors: {}", errors.len());
    for e in &errors {
        eprintln!("  ! {}", e);
    }
    if !errors.is_empty() {
        std::process::exit(1);
    }
}

fn walk_md(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_md(&path)?);
        } else if path.extension().and_then(|s| s.to_str()) == Some("md") {
            // Skip .gitkeep and README
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem != "README" {
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn migrate_file(
    path: &Path,
    baseline_sha: &str,
    dry_run: bool,
) -> Result<bool, Box<dyn std::error::Error>> {
    let body = fs::read_to_string(path)?;
    if has_f0_frontmatter(&body) {
        return Ok(false);
    }
    let new_body = synthesize_frontmatter(path, &body, baseline_sha)?;
    if !dry_run {
        fs::write(path, new_body)?;
    }
    Ok(true)
}

fn has_f0_frontmatter(body: &str) -> bool {
    if !body.starts_with("---\n") {
        return false;
    }
    let after = &body[4..];
    let end = match after.find("\n---\n") {
        Some(i) => i,
        None => return false,
    };
    let fm = &after[..end];
    fm.contains("\nid:") && fm.contains("\nstatus:") && fm.contains("\nfinding_kind:")
}

fn synthesize_frontmatter(
    path: &Path,
    body: &str,
    baseline_sha: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let title = body
        .lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l.trim_start_matches("# ").to_string())
        .unwrap_or_else(|| {
            path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("untitled")
                .to_string()
        });

    let category = guess_category(&title, body);
    let finding_kind = "legacy-uncategorized";
    let (file_loc, symbol_loc) = guess_location(body);
    let (kind, area) = if file_loc.is_some() {
        ("structural", None)
    } else {
        ("process", Some(slugify(&title)))
    };

    let location_part = match (kind, &file_loc, &symbol_loc, &area) {
        ("structural", Some(f), Some(s), _) => format!("{}::{}", f, s),
        ("structural", Some(f), None, _) => format!("{}::unknown", f),
        (_, _, _, Some(a)) => format!("area::{}", a),
        _ => "area::unknown".to_string(),
    };
    let id = format!("{}:{}:{}", category, location_part, finding_kind);

    let mut fm = String::new();
    fm.push_str("---\n");
    fm.push_str(&format!("id: {}\n", id));
    fm.push_str(&format!("category: {}\n", category));
    fm.push_str(&format!("kind: {}\n", kind));
    fm.push_str("location:\n");
    if kind == "structural" {
        fm.push_str(&format!("  file: {}\n", file_loc.as_deref().unwrap_or("")));
        fm.push_str(&format!(
            "  symbol: {}\n",
            symbol_loc.as_deref().unwrap_or("unknown")
        ));
        fm.push_str("  multi: false\n");
    } else {
        fm.push_str(&format!(
            "  area: {}\n",
            area.as_deref().unwrap_or("unknown")
        ));
        fm.push_str("  multi: false\n");
    }
    fm.push_str(&format!("finding_kind: {}\n", finding_kind));
    fm.push_str("phase_introduced: unknown\n");
    fm.push_str("status: open\n");
    fm.push_str("created_at: migration\n");
    fm.push_str("created_by: legacy-migration\n");
    fm.push_str(&format!("last_verified_at: {}\n", baseline_sha));
    fm.push_str("severity: medium\n");
    fm.push_str("do_not_autofix: false\n");
    fm.push_str("---\n\n");
    fm.push_str(body);
    Ok(fm)
}

fn guess_category(title: &str, body: &str) -> &'static str {
    let t = title.to_lowercase();
    let b = body.to_lowercase();
    let h = |s: &str| t.contains(s) || b.contains(s);
    if h("security") || h("vulnerab") || h("injection") || h("auth bypass") {
        "security"
    } else if h("duplicate") || h("redundant") {
        "duplicate"
    } else if h("dead code") || h("unused") {
        "dead-code"
    } else if h("layer") || h("boundary") {
        "layer-leak"
    } else if h("contract") || h("signature") {
        "contract-drift"
    } else if h("seam") {
        "seam-coverage"
    } else {
        "scope"
    }
}

fn guess_location(body: &str) -> (Option<String>, Option<String>) {
    for line in body.lines() {
        if let Some(file) = extract_file_path(line) {
            let symbol = extract_symbol_near(line);
            return (Some(file), symbol);
        }
    }
    (None, None)
}

fn extract_file_path(line: &str) -> Option<String> {
    for prefix in ["src/", "tests/", "examples/", "benches/"] {
        if let Some(idx) = line.find(prefix) {
            let rest = &line[idx..];
            let end = rest
                .find(|c: char| c.is_whitespace() || c == ':' || c == ',' || c == ')')
                .unwrap_or(rest.len());
            let p = &rest[..end];
            if p.ends_with(".rs") {
                return Some(p.to_string());
            }
        }
    }
    None
}

fn extract_symbol_near(line: &str) -> Option<String> {
    for tok in ["fn ", "struct ", "trait ", "enum ", "impl "] {
        if let Some(i) = line.find(tok) {
            let rest = &line[i + tok.len()..];
            let end = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
                .unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn current_main_sha() -> Result<String, Box<dyn std::error::Error>> {
    let out = Command::new("git").args(["rev-parse", "main"]).output()?;
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}
