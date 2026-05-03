//! Build script for trilithon-core.
//!
//! Emits an environment variable pointing to the schema output directory so
//! the `gen_mutation_schemas` binary can locate it at runtime.

fn main() {
    // Emit the path to the workspace-level schema directory.
    // CARGO_MANIFEST_DIR points to `core/crates/core/`; we go up three levels
    // to reach the project root, then into `docs/schemas/mutations/`.
    //
    // CARGO_MANIFEST_DIR is always set by cargo when running a build script.
    // We use `var_os` to avoid panicking via `expect`.
    if let Some(manifest_dir) = std::env::var_os("CARGO_MANIFEST_DIR") {
        let manifest_dir = manifest_dir.to_string_lossy();
        println!(
            "cargo:rustc-env=TRILITHON_SCHEMA_DIR={manifest_dir}/../../../docs/schemas/mutations"
        );
    }
    println!("cargo:rerun-if-changed=build.rs");
}
