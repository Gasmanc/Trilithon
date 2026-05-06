//! Build script for trilithon-core.
//!
//! Emits an environment variable pointing to the schema output directory so
//! the `gen_mutation_schemas` binary can locate it at runtime.

fn main() {
    // Compute the workspace root from CARGO_MANIFEST_DIR (core/crates/core/ → root).
    if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        // CARGO_MANIFEST_DIR = core/crates/core/; nth(3) steps up to the repo root.
        if let Some(workspace_root) = std::path::Path::new(&manifest_dir).ancestors().nth(3) {
            let schema_dir = workspace_root.join("docs/schemas/mutations");
            println!(
                "cargo:rustc-env=TRILITHON_SCHEMA_DIR={}",
                schema_dir.display()
            );
            // Re-run when schema files change so incremental builds detect drift
            // without requiring a full `just check` invocation.
            println!("cargo:rerun-if-changed={}", schema_dir.display());
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
}
