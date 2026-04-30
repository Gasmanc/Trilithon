#!/usr/bin/env bash
# Build the Rust ffi crate as an XCFramework consumable by the Swift app.
#
# Outputs:
#   target/xcframework/..xcframework
#   target/xcframework/Generated/  (Swift bindings)
#
# The Swift app imports the XCFramework as a binary target and the bindings
# as a sibling Swift module. See app/XCFRAMEWORK_INTEGRATION.txt.

set -euo pipefail

cd "$(dirname "$0")/.."

CRATE_NAME=".-ffi"
LIB_NAME="lib._ffi.a"
OUT_DIR="target/xcframework"
GEN_DIR="$OUT_DIR/Generated"

mkdir -p "$OUT_DIR" "$GEN_DIR"

# 1. Build for the Apple targets.
TARGETS=(
  aarch64-apple-ios            # iOS device
  aarch64-apple-ios-sim        # Apple-Silicon simulator
  x86_64-apple-ios             # Intel simulator (fallback)
  aarch64-apple-darwin         # macOS Apple-Silicon
  x86_64-apple-darwin          # macOS Intel
)

for tgt in "${TARGETS[@]}"; do
  if ! rustup target list --installed | grep -q "^$tgt$"; then
    echo "→ installing rustup target $tgt"
    rustup target add "$tgt"
  fi
  echo "→ cargo build --release --target $tgt -p $CRATE_NAME"
  cargo build --release --target "$tgt" -p "$CRATE_NAME"
done

# 2. Combine simulator slices into a single fat archive.
mkdir -p target/sim-universal
lipo -create \
  "target/aarch64-apple-ios-sim/release/$LIB_NAME" \
  "target/x86_64-apple-ios/release/$LIB_NAME" \
  -output "target/sim-universal/$LIB_NAME"

# 3. Combine macOS slices.
mkdir -p target/macos-universal
lipo -create \
  "target/aarch64-apple-darwin/release/$LIB_NAME" \
  "target/x86_64-apple-darwin/release/$LIB_NAME" \
  -output "target/macos-universal/$LIB_NAME"

# 4. Generate Swift bindings from the .udl.
echo "→ generating Swift bindings"
cargo run -p "$CRATE_NAME" --bin uniffi-bindgen --quiet -- \
  generate "crates/ffi/src/core.udl" \
  --language swift \
  --out-dir "$GEN_DIR" \
  --config "crates/ffi/uniffi.toml" 2>/dev/null || \
  cargo run -p uniffi-bindgen --bin uniffi-bindgen --quiet -- \
    generate "crates/ffi/src/core.udl" \
    --language swift \
    --out-dir "$GEN_DIR" \
    --config "crates/ffi/uniffi.toml"

# 5. Pack the XCFramework.
rm -rf "$OUT_DIR/..xcframework"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/release/$LIB_NAME" \
    -headers "$GEN_DIR" \
  -library "target/sim-universal/$LIB_NAME" \
    -headers "$GEN_DIR" \
  -library "target/macos-universal/$LIB_NAME" \
    -headers "$GEN_DIR" \
  -output "$OUT_DIR/..xcframework"

echo "✓ wrote $OUT_DIR/..xcframework"
echo "✓ wrote $GEN_DIR/*.swift bindings"
