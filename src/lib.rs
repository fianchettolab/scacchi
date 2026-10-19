//! Chess rules for Swift, built on shakmaty: a game from any FEN, its legal moves in UCI,
//! SAN for every move, and every ending, exported through UniFFI.

use std::sync::Mutex;

use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Position,
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

    /// Plays a UCI move and returns its SAN.
    pub fn play(&self, uci: String) -> Result<String, ScacchiError> {
        let mut state = self.state.lock().unwrap();
        let m = state.parse(&uci)?;
        let san = SanPlus::from_move(state.position.clone(), m).to_string();
        if state.position.is_irreversible(m) {
            state.hashes.clear();
        }
        state.position.play_unchecked(m);
        let hash = hash(&state.position);
        state.hashes.push(hash);
        Ok(san)
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
        moves.iter().map(|m| game.play(m.to_string()).unwrap()).collect()
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
        assert_eq!(game.play("e8g8".into()).unwrap(), "O-O");
        assert!(game.legal_moves().contains(&"e1g1".to_string()));

        let promo = Game::from_fen("8/P6k/8/8/8/8/8/K7 w - - 0 1".into()).unwrap();
        assert_eq!(promo.san("a7a8n".into()).unwrap(), "a8=N");
        assert_eq!(promo.play("a7a8q".into()).unwrap(), "a8=Q");
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
}
