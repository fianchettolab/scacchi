import Testing
import Scacchi

/// The bindings end to end: the same checks as the Rust tests, through Swift.
struct GameTests {
    @Test func startPosition() {
        let game = Game()
        #expect(game.legalMoves().count == 20)
        #expect(game.turn() == .white)
        #expect(game.fen() == "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
        #expect(game.status() == .ongoing)
    }

    @Test func sanForEnPassantCastlingAndPromotion() throws {
        let game = Game()
        let sans = try ["e2e4", "g8f6", "e4e5", "d7d5", "e5d6"].map { try game.play(uci: $0) }
        #expect(sans.last == "exd6")

        let castling = try Game.fromFen(fen: "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1")
        #expect(try castling.san(uci: "e1c1") == "O-O-O")
        #expect(try castling.play(uci: "e1g1") == "O-O")

        let promotion = try Game.fromFen(fen: "8/P6k/8/8/8/8/8/K7 w - - 0 1")
        #expect(try promotion.play(uci: "a7a8q") == "a8=Q")
    }

    @Test func checkmate() throws {
        let game = Game()
        for move in ["f2f3", "e7e5", "g2g4"] { _ = try game.play(uci: move) }
        #expect(try game.play(uci: "d8h4") == "Qh4#")
        #expect(game.status() == .checkmate(winner: .black))
        #expect(game.isCheck())
    }

    @Test func draw() throws {
        let stalemate = try Game.fromFen(fen: "7k/5Q2/6K1/8/8/8/8/8 b - - 0 1")
        #expect(stalemate.status() == .draw(reason: .stalemate))
    }

    @Test func errors() throws {
        #expect(throws: ScacchiError.self) { try Game.fromFen(fen: "not a fen") }
        #expect(throws: ScacchiError.IllegalMove(uci: "e2e5")) { try Game().play(uci: "e2e5") }
    }

    /// Shared across tasks: the generated class is Sendable and locks inside.
    @Test func concurrentUse() async {
        let game = Game()
        await withTaskGroup(of: Int.self) { group in
            for _ in 0..<8 { group.addTask { game.legalMoves().count } }
            for await count in group { #expect(count == 20) }
        }
    }
}
