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
- **Study chapters are PGN, as Lichess exports them** (`src/pgn.rs`): pgn-reader streams
  the events and Scacchi builds the tree and writes it back; read → write → read gives the
  same tree, not the same bytes.
  - **A `Chapter` object with node ids**, locked inside like `Game`; the root (id 0) is the
    start position. `node(id:)` returns a record; edits (`addMove`, `setComments`,
    `setAnnotations`, `setShapes`, `setTag`) go through the object, so legality and SAN
    stay in Rust. `addMove` returns the existing child for a move already there. Ids are
    never reused. Deleting and promoting variations aren't there yet.
  - **Comments are lists, as in chessops:** after the move, one per `{…}`; before it only
    at a variation's first move (`startingComments`, kept from imports); the root's are
    the chapter's opening comment; a comment after `)` belongs to the replaced move.
  - **Annotations:** `moveMark` ($1–$6, written as symbols on the move, `Nf6!?`),
    `positionSign` (Lichess's eight: $10, $13–$19) and `others` in order; a second mark or
    sign goes to the others. `others` are `UInt32`s, as `[UInt8]` would be `Data`.
  - **Shapes belong to the node**, read from any comment with chessops' syntax and written
    in order (a new `%csl`/`%cal` each time the kind changes). Other `[%…]` commands are
    kept raw in `commands` and written back after the shapes, in the last comment.
  - **The clock is the node's too,** as in chessops: `clock`, the mover's time left in
    seconds, from `[%clk H:MM:SS]` in any comment, the last one winning (the root's from
    the opening comment), and set with `setClock`. Within chessops' limits, so a clock
    always reads back as written: 1–5 digits of hours, 1–2 of minutes and seconds, up to
    three decimals, at most 99999:59:59.999; outside them a `[%clk]` stays raw in
    `commands`, and `setClock` refuses it. `setClock` keeps the millisecond, as it's
    written. Written last, after the other commands, as `H:MM:SS` with decimals only when
    there are any (`0:00:09.4`). Reading trims spaces chessops wouldn't. `Node` is
    `PartialEq` only, for the `f64`.
  - **Tags** are kept in order, except FEN and SetUp, which come from the root position
    and are written when it isn't the standard start. A Variant other than standard is an
    error. A movetext result with no Result tag becomes one.
  - **Strict:** an illegal move, an unmatched bracket, a variation with nothing to replace
    or a bad FEN fails the whole read, naming the chapter (from 1) and the move. So does
    syntax pgn-reader can't get past (it reports an I/O error, such as an unterminated
    tag). Tokens pgn-reader doesn't recognise are skipped by it, not reported: `O—O` with
    an em dash just ends the line.
  - **Tag values are unescaped by Scacchi,** not pgn-reader's `RawTag::decode`: in 0.29.1
    it turns `\\\\` into one backslash instead of two.
  - **Tests** port chessops' PGN cases and its `data/` files (`src/pgn/tests/chessops/`,
    GPL-3.0-or-later, origin in its README), noting where Scacchi differs.
  - Swift names avoid clashes with SwiftUI and Swift Testing: `DrawShape`, `PgnTag`.
- **No byte vectors in the API.** UniFFI turns a Rust `Vec<u8>` into Swift `Data`, so
  return records (`Piece`) or lists of them.
- **Pinned exactly:** shakmaty, pgn-reader and UniFFI (`=` versions in `Cargo.toml`).
  Upgrading any is a deliberate change, tested on both sides; pgn-reader follows
  shakmaty's minor version.
- **`shakmatyVersion()` reports the upstream** (apps show it next to Scacchi's own
  version). `build.rs` reads it from `Cargo.lock`, so it's what was linked. The Rust test
  checks it against the `=` pin; the Swift test spells the version out, so a shakmaty
  upgrade updates it too.
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
