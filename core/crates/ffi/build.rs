#![allow(missing_docs)]
#![allow(clippy::expect_used)]
#![allow(clippy::disallowed_methods)]

/// Build script for uniffi FFI bindings.
fn main() {
    uniffi_build::generate_scaffolding("src/core.udl").expect("uniffi scaffolding");
}
