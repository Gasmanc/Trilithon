//! Integration test: schema files in `docs/schemas/mutations/` must match
//! what `gen_mutation_schemas` would generate.
//!
//! If this test fails it means the Mutation types changed but the schema files
//! were not regenerated. Fix by running:
//!   `cargo run -p trilithon-core --bin gen_mutation_schemas`

#![allow(clippy::expect_used, clippy::disallowed_methods)]
// reason: test-only code; panics are the correct failure mode in tests

#[test]
fn schemas_match_committed() {
    // Locate the workspace root by walking up from CARGO_MANIFEST_DIR.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = std::path::Path::new(manifest_dir)
        .ancestors()
        .nth(3) // core/crates/core → core/crates → core → workspace root
        .expect("workspace root must be reachable from CARGO_MANIFEST_DIR");

    // Run the schema generator.
    let gen_status = std::process::Command::new("cargo")
        .args([
            "run",
            "-p",
            "trilithon-core",
            "--bin",
            "gen_mutation_schemas",
        ])
        .current_dir(workspace_root.join("core"))
        .status()
        .expect("cargo run gen_mutation_schemas must not fail to spawn");

    assert!(
        gen_status.success(),
        "gen_mutation_schemas exited with non-zero status — schema generation failed"
    );

    // Check that the schema files are unchanged after generation.
    let diff_status = std::process::Command::new("git")
        .args(["diff", "--exit-code", "docs/schemas/mutations/"])
        .current_dir(workspace_root)
        .status()
        .expect("git diff must not fail to spawn");

    assert!(
        diff_status.success(),
        "Schema files have drifted from the committed versions. \
         Run `cargo run -p trilithon-core --bin gen_mutation_schemas` and commit the result."
    );
}
