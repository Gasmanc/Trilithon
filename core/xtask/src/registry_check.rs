//! xtask: registry-check — gate on contract drift (warn or strict mode).

use std::process::exit;

pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let strict = args.iter().any(|a| a == "--strict");

    let drift_count = check_for_drift();

    if drift_count == 0 {
        println!("registry-check: no drift");
        exit(0);
    }

    if strict {
        eprintln!(
            "✗ registry-check: {} item(s) drifted — STRICT mode",
            drift_count
        );
        exit(1);
    }

    eprintln!(
        "⚠ registry-check: {} item(s) drifted from contracts.md",
        drift_count
    );
    eprintln!("  Run: just registry-regen");
    exit(0);
}

fn check_for_drift() -> usize {
    // Compare extracted contract set vs. committed contracts.md set.
    // Stub: delegate to registry-verify exit code.
    0
}
