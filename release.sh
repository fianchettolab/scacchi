#!/usr/bin/env bash
# Prepares release <version>: builds the XCFramework, zips it, writes the version and the
# zip's checksum into Package.swift, commits and tags. Then push the tag and upload the zip:
#   git push origin main <version>
#   gh release create <version> build/ScacchiFFI.xcframework.zip --title <version>
set -euo pipefail

version="${1:?usage: release.sh <version>, e.g. 0.1.0}"
here="$(cd "$(dirname "$0")" && pwd)"
cd "$here"

if [[ -n "$(git status --porcelain)" ]]; then
    echo "The working tree has changes; commit them first." >&2
    exit 1
fi

./build-xcframework.sh
if [[ -n "$(git status --porcelain Sources/)" ]]; then
    echo "The generated bindings changed; commit them first." >&2
    exit 1
fi

zip="build/ScacchiFFI.xcframework.zip"
rm -f "$zip"
(cd build && ditto -c -k --keepParent ScacchiFFI.xcframework ScacchiFFI.xcframework.zip)
checksum="$(swift package compute-checksum "$zip")"

sed -i '' "s/^let release = .*/let release = (version: \"$version\", checksum: \"$checksum\")/" Package.swift
sed -i '' "s/^version = \".*\"/version = \"$version\"/" Cargo.toml
cargo metadata --format-version 1 >/dev/null # refreshes Cargo.lock's version

git add Package.swift Cargo.toml Cargo.lock
git commit -m "Release $version"
git tag "$version"

echo "Tagged $version ($(du -h "$zip" | cut -f1), checksum $checksum)."
echo "Next: git push origin main $version"
echo "      gh release create $version $zip --title $version"
