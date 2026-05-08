//! xtask: audit-duplicates — structural duplicate detector and seam-stub checker.

use std::process::exit;

pub fn run() {
    let args: Vec<String> = std::env::args().skip(2).collect();
    let check_seam_stubs = args.iter().any(|a| a == "--check-seam-stubs");

    let findings_count = if check_seam_stubs {
        check_seam_stub_asserts()
    } else {
        scan_struct_duplicates() + scan_trait_duplicates() + scan_enum_duplicates()
    };

    if findings_count > 0 {
        eprintln!("audit-duplicates: {} finding(s)", findings_count);
        exit(1);
    }
    println!("audit-duplicates: clean (template stub)");
}

fn scan_struct_duplicates() -> usize {
    0
}
fn scan_trait_duplicates() -> usize {
    0
}
fn scan_enum_duplicates() -> usize {
    0
}
fn check_seam_stub_asserts() -> usize {
    0
}
