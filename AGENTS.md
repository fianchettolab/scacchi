# AGENTS.md

Scacchi is part of Fianchetto Lab. Handoff notes for anyone (human or agent) picking up
Scacchi cold: how to work here and the decisions already settled with the owner. What the
package does, its layout and how to build it are in [README.md](README.md); this file
doesn't repeat them.

## Working with the owner

- **Advise before applying** anything non-trivial (API changes, new dependencies,
  structural refactors): lay out the options and a recommendation, then wait for a
  go-ahead.
- **Let the owner test before committing.** Don't commit until they say so.
- **Releases and pushes need a go-ahead.** A release is public the moment it's created,
  and apps download it.
- **Record settled decisions here**, as the current state, not as a history of changes.
- **Commits** use the owner's GitHub noreply address (set in the global git config); never
  their personal email.

## Build and test

- Rust 1.97 or later (shakmaty's minimum), with the targets `aarch64-apple-ios`,
  `aarch64-apple-ios-sim` and `aarch64-apple-darwin` (`build-xcframework.sh` adds them).
- `./build-xcframework.sh`, then `cargo test` and `swift test`. The first build takes
  about 5 minutes; later ones about 30 s. Keep builds free of warnings, in Rust and in the
  Swift 6 language mode.
- **A Rust API change regenerates `Sources/Scacchi/Scacchi.swift`.** Commit it with the
  change: Swift users never run Rust, so that file is what they compile.
- Test both sides: a Rust test in `src/lib.rs` and a Swift test in `Tests/ScacchiTests/`
  for anything new in the API.

## Settled decisions

- **One `Game` object**, from the standard start or any FEN. It holds its state behind a
  `Mutex`, so the generated Swift class is `Sendable`. No undo: a caller replays from the
  start position.
- **Moves in UCI,** castling written as the king's two-square move (`e1g1`), as Lichess's
  puzzles and Maia write it; the king-takes-rook form (`e1h1`) is accepted too. SAN
  includes `+` and `#`.
- **Squares are numbered rank-first:** a1 = 0, b1 = 1 … h8 = 63, as in Maia's encoding.
  `board()` and `checkers()` list pieces from a1 up.
- **Endings** come from `status()`, in this order: checkmate, stalemate, insufficient
  material (shakmaty's rule), threefold repetition, the fifty-move rule. Repetition counts
  the zobrist hashes of the positions since the last irreversible move; the third
  occurrence is a draw. Draws are reported, not claimed: the caller decides.
- **No byte vectors in the API.** UniFFI turns a Rust `Vec<u8>` into Swift `Data`, so
  return records (`Piece`) or lists of them.
- **Pinned exactly:** shakmaty and UniFFI (`=` versions in `Cargo.toml`). Upgrading
  either is a deliberate change, tested on both sides.
- **UniFFI's `cli` feature stays off the library**, on the `uniffi-bindgen` binary only
  (`required-features`). On the library it linked the whole bindings generator in and
  took the XCFramework from 62 MB to 176 MB.
- **Apple silicon only:** iOS device, iOS simulator and macOS, arm64; iOS 17 and macOS 14
  minimums.
- **`Package.swift` picks the binary:** a local `build/ScacchiFFI.xcframework` when there
  is one, otherwise the release URL with its checksum, both written by `release.sh`.
- **Licence:** GPL-3.0-or-later, like shakmaty. `THIRD_PARTY_LICENSES.txt` lists every
  crate linked into the library with its licence; re-run `./notices.py` after any
  dependency change and commit the result. Crates that ship no licence file get their
  text from `LICENSE` (GPL) or `licenses/` (MPL-2.0); `notices.py` stops if one is
  missing.

## Releasing

1. On a clean `main`: `./release.sh <version>`. It builds, zips, writes the version and
   checksum into `Package.swift` and `Cargo.toml`, commits "Release <version>" and tags.
2. With the owner's go-ahead: `git push origin main <version>`, then
   `gh release create <version> build/ScacchiFFI.xcframework.zip --title <version>`.
3. Users raise their requirement to pick it up.

Versions follow semantic versioning; while it's 0.x, a minor version may break the API.
Doing this through CI and release PRs is planned.

## Gotchas

- **SwiftPM caches the manifest:** after creating or deleting `build/`, touch
  `Package.swift` (or reset package caches in Xcode) before the switch between the local
  build and the release takes effect.
- **`release.sh` refuses** a dirty tree, and generated bindings that differ from the
  committed ones.
- **Rust's `target/` is large** (several GB); it's gitignored and safe to delete.
