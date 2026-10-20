//! Chess rules for Swift, built on shakmaty: a game from any FEN, its legal moves in UCI,
//! SAN for every move, and every ending, exported through UniFFI.

use std::sync::Mutex;

use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Position, Role as ShakmatyRole,
    fen::Fen,
    san::SanPlus,
    uci::UciMove,
    zobrist::Zobrist64,
};

uniffi::setup_scaffolding!();

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Side {
    White,
    Black,
}

impl From<Color> for Side {
    fn from(color: Color) -> Self {
        match color {
            Color::White => Side::White,
            Color::Black => Side::Black,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Role {
    Pawn,
    Knight,
    Bishop,
    Rook,
    Queen,
    King,
}

impl From<ShakmatyRole> for Role {
    fn from(role: ShakmatyRole) -> Self {
        match role {
            ShakmatyRole::Pawn => Role::Pawn,
            ShakmatyRole::Knight => Role::Knight,
            ShakmatyRole::Bishop => Role::Bishop,
            ShakmatyRole::Rook => Role::Rook,
            ShakmatyRole::Queen => Role::Queen,
            ShakmatyRole::King => Role::King,
        }
    }
}

/// A piece on its square. Squares run rank-first: a1 = 0, b1 = 1 … h1 = 7, a2 = 8 … h8 = 63.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct Piece {
    pub square: u8,
    pub color: Side,
    pub role: Role,
}

/// What's on the board and who moves.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct BoardState {
    /// By square, a1 first.
    pub pieces: Vec<Piece>,
    pub turn: Side,
}

/// A move just played.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PlayedMove {
    /// In UCI, castling as the king's two-square move (e1g1).
    pub uci: String,
    /// With + or #.
    pub san: String,
    /// En passant included.
    pub is_capture: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum DrawReason {
    Stalemate,
    InsufficientMaterial,
    /// The same position (side to move, castling and en passant rights included) a third time.
    ThreefoldRepetition,
    /// A hundred plies without a capture or a pawn move.
    FiftyMoveRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GameStatus {
    Ongoing,
    Checkmate { winner: Side },
    Draw { reason: DrawReason },
}

#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum ScacchiError {
    #[error("invalid FEN {fen}: {reason}")]
    InvalidFen { fen: String, reason: String },
    #[error("illegal move {uci}")]
    IllegalMove { uci: String },
}

struct State {
    position: Chess,
    /// Zobrist hashes of the positions since the last irreversible move, current one last.
    /// Positions before a capture or a pawn move can't come back, so they're dropped.
    hashes: Vec<u64>,
}

impl State {
    fn new(position: Chess) -> Self {
        let hash = hash(&position);
        State { position, hashes: vec![hash] }
    }

    fn parse(&self, uci: &str) -> Result<shakmaty::Move, ScacchiError> {
        let illegal = || ScacchiError::IllegalMove { uci: uci.to_owned() };
        uci.parse::<UciMove>()
            .map_err(|_| illegal())?
            .to_move(&self.position)
            .map_err(|_| illegal())
    }
}

fn hash(position: &Chess) -> u64 {
    position.zobrist_hash::<Zobrist64>(EnPassantMode::Legal).0
}

fn uci(m: shakmaty::Move) -> String {
    m.to_uci(CastlingMode::Standard).to_string()
}

/// A game from a start position. Thread-safe: every method takes the lock.
#[derive(uniffi::Object)]
pub struct Game {
    state: Mutex<State>,
}

#[uniffi::export]
impl Game {
    /// The standard start position.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Game { state: Mutex::new(State::new(Chess::default())) }
    }

    #[uniffi::constructor]
    pub fn from_fen(fen: String) -> Result<Self, ScacchiError> {
        let invalid = |reason: String| ScacchiError::InvalidFen { fen: fen.clone(), reason };
        let position: Chess = Fen::from_ascii(fen.as_bytes())
            .map_err(|e| invalid(e.to_string()))?
            .into_position(CastlingMode::Standard)
            .map_err(|e| invalid(e.to_string()))?;
        Ok(Game { state: Mutex::new(State::new(position)) })
    }

    pub fn fen(&self) -> String {
        let state = self.state.lock().unwrap();
        Fen::from_position(&state.position, EnPassantMode::Legal).to_string()
    }

    pub fn turn(&self) -> Side {
        self.state.lock().unwrap().position.turn().into()
    }

    pub fn is_check(&self) -> bool {
        self.state.lock().unwrap().position.is_check()
    }

    /// The move number, from the FEN's last field: 1 at the start, up by one after Black moves.
    pub fn fullmoves(&self) -> u32 {
        self.state.lock().unwrap().position.fullmoves().get()
    }

    pub fn board(&self) -> BoardState {
        let state = self.state.lock().unwrap();
        let position = &state.position;
        let pieces = position
            .board()
            .iter()
            .map(|(square, piece)| Piece {
                square: square.to_u32() as u8,
                color: piece.color.into(),
                role: piece.role.into(),
            })
            .collect();
        BoardState { pieces, turn: position.turn().into() }
    }

    /// The pieces giving check, a1 first; empty when not in check.
    pub fn checkers(&self) -> Vec<Piece> {
        let state = self.state.lock().unwrap();
        let board = state.position.board();
        state
            .position
            .checkers()
            .into_iter()
            .filter_map(|square| {
                board.piece_at(square).map(|piece| Piece {
                    square: square.to_u32() as u8,
                    color: piece.color.into(),
                    role: piece.role.into(),
                })
            })
            .collect()
    }

    /// Legal moves in UCI, castling as the king's two-square move (e1g1).
    pub fn legal_moves(&self) -> Vec<String> {
        let state = self.state.lock().unwrap();
        state.position.legal_moves().into_iter().map(uci).collect()
    }

    /// The move's SAN in the current position, with + or #, without playing it.
    pub fn san(&self, uci: String) -> Result<String, ScacchiError> {
        let state = self.state.lock().unwrap();
        let m = state.parse(&uci)?;
        Ok(SanPlus::from_move(state.position.clone(), m).to_string())
    }

    /// Plays a UCI move.
    pub fn play(&self, uci: String) -> Result<PlayedMove, ScacchiError> {
        let mut state = self.state.lock().unwrap();
        let m = state.parse(&uci)?;
        let played = PlayedMove {
            uci: self::uci(m),
            san: SanPlus::from_move(state.position.clone(), m).to_string(),
            is_capture: m.is_capture(),
        };
        if state.position.is_irreversible(m) {
            state.hashes.clear();
        }
        state.position.play_unchecked(m);
        let hash = hash(&state.position);
        state.hashes.push(hash);
        Ok(played)
    }

    pub fn status(&self) -> GameStatus {
        let state = self.state.lock().unwrap();
        let position = &state.position;
        if position.is_checkmate() {
            return GameStatus::Checkmate { winner: (!position.turn()).into() };
        }
        let reason = if position.is_stalemate() {
            Some(DrawReason::Stalemate)
        } else if position.is_insufficient_material() {
            Some(DrawReason::InsufficientMaterial)
        } else if state.hashes.iter().filter(|&&h| Some(&h) == state.hashes.last()).count() >= 3 {
            Some(DrawReason::ThreefoldRepetition)
        } else if position.halfmoves() >= 100 {
            Some(DrawReason::FiftyMoveRule)
        } else {
            None
        };
        reason.map_or(GameStatus::Ongoing, |reason| GameStatus::Draw { reason })
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn play_all(game: &Game, moves: &[&str]) -> Vec<String> {
        moves.iter().map(|m| game.play(m.to_string()).unwrap().san).collect()
    }

    #[test]
    fn start_position() {
        let game = Game::new();
        assert_eq!(game.legal_moves().len(), 20);
        assert_eq!(game.turn(), Side::White);
        assert_eq!(game.fen(), "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        assert_eq!(game.status(), GameStatus::Ongoing);
    }

    #[test]
    fn san_en_passant_castling_promotion() {
        let game = Game::new();
        let sans = play_all(&game, &["e2e4", "g8f6", "e4e5", "d7d5", "e5d6", "e7e6", "g1f3", "f8e7", "f1e2"]);
        assert_eq!(sans[4], "exd6");
        assert_eq!(game.play("e8g8".into()).unwrap().san, "O-O");
        assert!(game.legal_moves().contains(&"e1g1".to_string()));

        let promo = Game::from_fen("8/P6k/8/8/8/8/8/K7 w - - 0 1".into()).unwrap();
        assert_eq!(promo.san("a7a8n".into()).unwrap(), "a8=N");
        assert_eq!(promo.play("a7a8q".into()).unwrap().san, "a8=Q");
    }

    #[test]
    fn checkmate() {
        let game = Game::new();
        let sans = play_all(&game, &["f2f3", "e7e5", "g2g4", "d8h4"]);
        assert_eq!(sans[3], "Qh4#");
        assert_eq!(game.status(), GameStatus::Checkmate { winner: Side::Black });
        assert!(game.legal_moves().is_empty());
    }

    #[test]
    fn draws() {
        let stalemate = Game::from_fen("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1".into()).unwrap();
        assert_eq!(stalemate.status(), GameStatus::Draw { reason: DrawReason::Stalemate });

        let bare = Game::from_fen("8/8/4k3/8/8/3K4/8/8 w - - 0 1".into()).unwrap();
        assert_eq!(bare.status(), GameStatus::Draw { reason: DrawReason::InsufficientMaterial });

        let fifty = Game::from_fen("8/8/4k3/8/8/3K4/8/R7 w - - 99 80".into()).unwrap();
        assert_eq!(fifty.status(), GameStatus::Ongoing);
        fifty.play("a1a2".into()).unwrap();
        assert_eq!(fifty.status(), GameStatus::Draw { reason: DrawReason::FiftyMoveRule });

        let game = Game::new();
        play_all(&game, &["g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1"]);
        assert_eq!(game.status(), GameStatus::Ongoing);
        game.play("f6g8".into()).unwrap();
        assert_eq!(game.status(), GameStatus::Draw { reason: DrawReason::ThreefoldRepetition });
    }

    #[test]
    fn errors() {
        assert!(matches!(Game::from_fen("not a fen".into()), Err(ScacchiError::InvalidFen { .. })));
        let game = Game::new();
        for bad in ["e2e5", "e7e5", "zz", "e2e4q", ""] {
            assert_eq!(game.play(bad.into()), Err(ScacchiError::IllegalMove { uci: bad.into() }));
        }
    }

    #[test]
    fn played_moves() {
        let game = Game::from_fen("4k3/8/8/3pP3/8/8/8/4K2R w K d6 0 1".into()).unwrap();
        let ep = game.play("e5d6".into()).unwrap();
        assert_eq!(ep, PlayedMove { uci: "e5d6".into(), san: "exd6".into(), is_capture: true });
        assert!(!game.play("e8d8".into()).unwrap().is_capture);
        assert_eq!(game.play("e1h1".into()).unwrap().uci, "e1g1"); // king takes rook, written e1g1
    }

    #[test]
    fn board_and_checkers() {
        let game = Game::new();
        let board = game.board();
        assert_eq!(board.pieces.len(), 32);
        assert_eq!(board.turn, Side::White);
        assert_eq!(board.pieces[0], Piece { square: 0, color: Side::White, role: Role::Rook });
        assert_eq!(board.pieces[4], Piece { square: 4, color: Side::White, role: Role::King });
        assert_eq!(board.pieces.last().unwrap(), &Piece { square: 63, color: Side::Black, role: Role::Rook });
        assert!(game.checkers().is_empty());
        assert_eq!(game.fullmoves(), 1);

        // Double check: knight on f6 and rook on e1 against the king on e8.
        let double = Game::from_fen("4k3/8/5N2/8/8/8/8/K3R3 b - - 0 40".into()).unwrap();
        assert_eq!(
            double.checkers(),
            vec![
                Piece { square: 4, color: Side::White, role: Role::Rook },
                Piece { square: 45, color: Side::White, role: Role::Knight },
            ]
        );
        assert_eq!(double.fullmoves(), 40);
    }
}
