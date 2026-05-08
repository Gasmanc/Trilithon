//! xtask: invariant-check — verify every symbol cited in contracts-invariants.md
//! exists in contracts.md.

use std::collections::BTreeSet;
use std::fs;
use std::process::exit;

const CONTRACTS: &str = "docs/architecture/contracts.md";
const INVARIANTS: &str = "docs/architecture/contracts-invariants.md";

pub fn run() {
    let contracts = match fs::read_to_string(CONTRACTS) {
        Ok(s) => s,
        Err(_) => {
            eprintln!("contracts.md missing — run `cargo xtask registry-extract --write`");
            exit(2);
        }
    };
    let invariants = match fs::read_to_string(INVARIANTS) {
        Ok(s) => s,
        Err(_) => {
            println!("invariant-check: no contracts-invariants.md (OK)");
            exit(0);
        }
    };

    let registered = extract_registered_symbols(&contracts);
    let cited = extract_cited_symbols(&invariants);

    let orphans: Vec<(String, usize)> = cited
        .into_iter()
        .filter(|(sym, _)| !registered.contains(sym))
        .collect();

    if orphans.is_empty() {
        println!(
            "invariant-check: clean ({} registered symbols)",
            registered.len()
        );
        exit(0);
    }

    eprintln!("✗ invariant-check: {} orphaned reference(s)", orphans.len());
    for (sym, line) in &orphans {
        eprintln!(
            "  contracts-invariants.md:{}: `{}` not in contracts.md",
            line, sym
        );
    }
    exit(1);
}

fn extract_registered_symbols(contracts: &str) -> BTreeSet<String> {
    // Walk headings like `## \`crate::path::Sym\``
    contracts
        .lines()
        .filter(|l| l.starts_with("## `"))
        .map(|l| {
            l.trim_start_matches("## `")
                .trim_end_matches('`')
                .to_string()
        })
        .collect()
}

fn extract_cited_symbols(invariants: &str) -> Vec<(String, usize)> {
    invariants
        .lines()
        .enumerate()
        .filter(|(_, l)| l.starts_with("## `"))
        .map(|(i, l)| {
            (
                l.trim_start_matches("## `")
                    .trim_end_matches('`')
                    .to_string(),
                i + 1,
            )
        })
        .collect()
}
