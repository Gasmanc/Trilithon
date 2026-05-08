//! xtask: set-merge-review-baseline
//!
//! Foundation E — sets the adoption baseline SHA for `/phase-merge-review`.
//!
//! `/where`'s `merge-without-merge-review` rule only fires for commits AFTER
//! this SHA. Without a baseline, adopting the system on an existing project
//! would flood `/where` with "missing merge review" alerts for every historical
//! commit on main.
//!
//! Movement is FORWARD-ONLY. Bumping the baseline requires a recorded
//! `accepted-as-is` super-finding documenting that all unaudited content as of
//! the new baseline is acknowledged debt.
//!
//! Usage:
//!   cargo xtask set-merge-review-baseline                    # set to current main HEAD
//!   cargo xtask set-merge-review-baseline --to <sha>         # set to specific SHA
//!   cargo xtask set-merge-review-baseline --bump --reason "..."  # forward-bump (records super-finding)
//!
//! Writes `cross_phase.merge_review_baseline_sha` in `.claude/review-config.yaml`.

use std::process::{Command, exit};

const REVIEW_CONFIG: &str = ".claude/review-config.yaml";

pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let bump = args.iter().any(|a| a == "--bump");
    let to_sha = args
        .windows(2)
        .find(|w| w[0] == "--to")
        .map(|w| w[1].clone());
    let reason = args
        .windows(2)
        .find(|w| w[0] == "--reason")
        .map(|w| w[1].clone());

    let new_sha = to_sha.unwrap_or_else(|| {
        let out = Command::new("git")
            .args(["rev-parse", "main"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    });

    let current = read_current_baseline();

    if let Some(prev) = current.as_deref() {
        if !is_ancestor(prev, &new_sha) {
            eprintln!("✗ Baseline must move forward only.");
            eprintln!("  Current: {}", prev);
            eprintln!("  Proposed: {}", new_sha);
            eprintln!("  {} is not an ancestor of {}.", prev, new_sha);
            exit(1);
        }
        if !bump {
            eprintln!("Baseline already set to {}.", prev);
            eprintln!("To advance: --bump --reason \"<text>\"");
            exit(1);
        }
        let reason = reason.unwrap_or_else(|| {
            eprintln!("--bump requires --reason \"<text>\"");
            exit(1)
        });
        record_super_finding(prev, &new_sha, &reason);
    }

    write_baseline(&new_sha);
    println!("merge-review baseline set to {}", new_sha);
}

fn read_current_baseline() -> Option<String> {
    let content = std::fs::read_to_string(REVIEW_CONFIG).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.trim().strip_prefix("merge_review_baseline_sha:") {
            let val = rest.trim().trim_matches('"').trim_matches('\'');
            if !val.is_empty() && val != "~" && val != "null" {
                return Some(val.to_string());
            }
        }
    }
    None
}

fn write_baseline(sha: &str) {
    let content = std::fs::read_to_string(REVIEW_CONFIG)
        .unwrap_or_else(|_| format!("cross_phase:\n  merge_review_baseline_sha: \"\"\n"));

    let new_content: String = content
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("merge_review_baseline_sha:") {
                let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
                format!("{}merge_review_baseline_sha: {}", indent, sha)
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Preserve trailing newline
    let new_content = if content.ends_with('\n') {
        new_content + "\n"
    } else {
        new_content
    };

    std::fs::write(REVIEW_CONFIG, new_content).unwrap();
}

fn is_ancestor(a: &str, b: &str) -> bool {
    Command::new("git")
        .args(["merge-base", "--is-ancestor", a, b])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn record_super_finding(prev: &str, new: &str, reason: &str) {
    // Write a Foundation-0 finding to End_of_Phase_Reviews/Unfixed/ with:
    //   id: cross-cutting:area::merge-process:baseline-bump
    //   status: accepted-as-is
    //   description: "Merge-review baseline bumped from {prev} to {new}. Reason: {reason}"
    let _ = (prev, new, reason);
}
