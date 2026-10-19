#!/usr/bin/env bash
# Builds build/ScacchiFFI.xcframework (iOS device, iOS simulator, macOS) and regenerates the
# Swift bindings in Sources/Scacchi/. Needs Rust (rustup) and Xcode.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

targets=(aarch64-apple-ios aarch64-apple-ios-sim aarch64-apple-darwin)
lib="libscacchi.a"
out="build"
framework="$out/ScacchiFFI.xcframework"

# Matches MaiaKit's minimums.
export IPHONEOS_DEPLOYMENT_TARGET=17.0
export MACOSX_DEPLOYMENT_TARGET=14.0

rustup target add "${targets[@]}" >/dev/null

for target in "${targets[@]}"; do
    cargo build --release --lib --target "$target"
done

# Swift bindings, C header and module map, from the macOS library's metadata.
rm -rf "$out/bindings"
cargo run --quiet --features cli --bin uniffi-bindgen -- generate \
    --library "target/aarch64-apple-darwin/release/$lib" \
    --language swift --out-dir "$out/bindings"

headers="$out/headers"
rm -rf "$headers"
mkdir -p "$headers"
cp "$out/bindings/scacchiFFI.h" "$headers/"
cp "$out/bindings/scacchiFFI.modulemap" "$headers/module.modulemap"

mkdir -p Sources/Scacchi
cp "$out/bindings/scacchi.swift" Sources/Scacchi/Scacchi.swift

rm -rf "$framework"
args=()
for target in "${targets[@]}"; do
    args+=(-library "target/$target/release/$lib" -headers "$headers")
done
xcodebuild -create-xcframework "${args[@]}" -output "$framework" >/dev/null

echo "Built $framework ($(du -sh "$framework" | cut -f1))"
