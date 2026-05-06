//! Schema generator binary.
//!
//! Writes one JSON Schema file per `Mutation` variant under
//! `docs/schemas/mutations/`, plus a root `Mutation.json` that captures the
//! full discriminated union.
//!
//! Run via:
//!   `cargo run -p trilithon-core --features schema --bin gen_mutation_schemas`

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use schemars::schema_for;
use serde_json::Value;
use trilithon_core::mutation::types::Mutation;

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let schema_dir = env!("TRILITHON_SCHEMA_DIR");
    let out = Path::new(schema_dir);

    fs::create_dir_all(out)?;

    // Root schema — full discriminated union.
    let root_schema = schema_for!(Mutation);
    let mut root_json = serde_json::to_string_pretty(&root_schema)?;
    root_json.push('\n');
    fs::write(out.join("Mutation.json"), root_json)?;

    // Per-variant stub schemas.  Each file contains a `$ref` to the root
    // schema plus a `description` for quick lookup.
    let variants: &[(&str, &str)] = &[
        ("CreateRoute", "Create a new route."),
        (
            "UpdateRoute",
            "Apply a partial update to an existing route.",
        ),
        ("DeleteRoute", "Remove a route."),
        ("CreateUpstream", "Create a new upstream."),
        (
            "UpdateUpstream",
            "Apply a partial update to an existing upstream.",
        ),
        ("DeleteUpstream", "Remove an upstream."),
        ("AttachPolicy", "Attach a policy preset to a route."),
        ("DetachPolicy", "Remove any policy attachment from a route."),
        (
            "UpgradePolicy",
            "Upgrade an attached policy to a newer preset version.",
        ),
        ("SetGlobalConfig", "Replace the global proxy configuration."),
        ("SetTlsConfig", "Replace the global TLS configuration."),
        (
            "ImportFromCaddyfile",
            "Merge routes and upstreams parsed from a Caddyfile.",
        ),
        (
            "Rollback",
            "Roll back desired state to a previous snapshot.",
        ),
    ];

    for (variant, description) in variants {
        // Build the stub as a BTreeMap to avoid serde_json::json! macro internals
        // triggering the clippy::disallowed_methods lint on unwrap().
        let mut stub: BTreeMap<&str, Value> = BTreeMap::new();
        stub.insert(
            "$schema",
            Value::String("http://json-schema.org/draft-07/schema#".to_owned()),
        );
        stub.insert(
            "$id",
            Value::String(format!(
                "https://trilithon.internal/schemas/mutations/{variant}.json"
            )),
        );
        stub.insert("title", Value::String((*variant).to_owned()));
        stub.insert("description", Value::String((*description).to_owned()));
        // $ref points to the root schema file directly (no fragment — the root
        // schema object IS the full union, not a definitions sub-key).
        stub.insert("$ref", Value::String("Mutation.json".to_owned()));

        let mut json = serde_json::to_string_pretty(&stub)?;
        json.push('\n');
        let filename = format!("{variant}.json");
        fs::write(out.join(&filename), json)?;
    }

    println!(
        "Generated {} schema files in {}",
        variants.len() + 1,
        out.display()
    );
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run()
}
