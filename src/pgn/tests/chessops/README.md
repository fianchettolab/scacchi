# chessops' PGN test data

The `.pgn` files here are copied unchanged from
[chessops](https://github.com/niklasf/chessops) (`data/`, commit `736c40c`), the
TypeScript library behind Lichess's frontend, by Niklas Fiekas and contributors. They're
licensed GPL-3.0-or-later, like Scacchi. The tests in `../chessops.rs` port the PGN and
comment cases from chessops' `src/pgn.test.ts`, adapted to Scacchi's stricter reader:
Scacchi checks every move and rejects variants and null moves.
