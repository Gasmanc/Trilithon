//! Smoke-test that the migration directory parses cleanly.

// sqlx::migrate::Migrator::new internally uses .expect() in its async body;
// clippy's MIR-level disallowed-methods lint fires at the call site even though
// this test never calls .expect() directly. Tests are permitted to do so per
// CLAUDE.md ("No unwrap()/expect() outside tests").
#[allow(clippy::disallowed_methods)]
#[tokio::test]
async fn initial_schema_parses() -> Result<(), Box<dyn std::error::Error>> {
    let migrations_dir = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations"));
    let migrator = sqlx::migrate::Migrator::new(migrations_dir).await?;
    let count = migrator.iter().count();
    if count != 9 {
        return Err(format!("expected exactly nine migration files, found {count}").into());
    }
    Ok(())
}
