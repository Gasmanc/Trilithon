//! xtask: registry-verify — verify contracts.md is up to date with source.

use std::process::{Command, exit};

pub fn run() {
    let extracted = match Command::new("cargo")
        .args(["xtask", "registry-extract"])
        .output()
    {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        Ok(o) => {
            eprintln!(
                "registry-extract failed:\n{}",
                String::from_utf8_lossy(&o.stderr)
            );
            exit(2);
        }
        Err(e) => {
            eprintln!("could not invoke registry-extract: {}", e);
            exit(2);
        }
    };

    let committed = match std::fs::read_to_string("docs/architecture/contracts.md") {
        Ok(s) => s,
        Err(_) => {
            eprintln!("contracts.md missing — run `cargo xtask registry-extract --write`");
            exit(1);
        }
    };

    if normalize(&extracted) == normalize(&committed) {
        println!("registry-verify: clean");
        exit(0);
    }

    eprintln!("registry-verify: DRIFT DETECTED");
    eprintln!("contracts.md does not match a fresh extract from current source.");
    eprintln!("Run: cargo xtask registry-extract --write");
    exit(1);
}

fn normalize(s: &str) -> String {
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .filter(|l| !l.starts_with("# Generated"))
        .collect::<Vec<_>>()
        .join("\n")
}
