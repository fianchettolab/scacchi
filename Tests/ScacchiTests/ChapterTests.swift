import Testing
import Scacchi

/// Study chapters through Swift: reading, walking, editing and writing, as the Rust tests do.
struct ChapterTests {
    static let study = """
        [Event "Openings: Italian"]
        [ChapterName "Italian"]
        [Orientation "black"]

        { The Italian. [%csl Ge4][%cal Ge2e4] } 1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 \
        (3... Nf6 $5 { Two knights. } 4. Ng5) 4. c3 $10 { [%clk 0:03:00] Quiet. } *
        """

    @Test func readsAChapter() throws {
        let chapters = try readStudy(pgn: Self.study)
        #expect(chapters.count == 1)
        let chapter = chapters[0]
        #expect(chapter.name() == "Italian")
        #expect(chapter.orientation() == .black)
        #expect(chapter.tags().first == PgnTag(name: "Event", value: "Openings: Italian"))

        let root = try chapter.node(id: chapter.root())
        #expect(root.played == nil)
        #expect(root.comments == ["The Italian."])
        #expect(root.shapes == [
            DrawShape(brush: .green, from: 28, to: nil),
            DrawShape(brush: .green, from: 12, to: 28),
        ])

        let main = try chapter.mainline(from: chapter.root())
        let sans = try main.map { try chapter.node(id: $0).played?.san }
        #expect(sans == ["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5", "c3"])

        let bc4 = try chapter.node(id: main[4])
        #expect(bc4.children.count == 2)
        let nf6 = try chapter.node(id: bc4.children[1])
        #expect(nf6.played?.uci == "g8f6")
        #expect(nf6.moveNumber == 3)
        #expect(nf6.side == .black)
        #expect(nf6.annotations.moveMark == .interesting)
        #expect(nf6.comments == ["Two knights."])

        let c3 = try chapter.node(id: main[6])
        #expect(c3.annotations == Annotations(moveMark: nil, positionSign: .equal, others: []))
        #expect(c3.comments == ["Quiet."])
        #expect(c3.clock == 180)
        #expect(c3.commands == [])
    }

    @Test func roundTrips() throws {
        let chapter = try readStudy(pgn: Self.study)[0]
        let pgn = chapter.pgn()
        let again = try readStudy(pgn: pgn)[0]
        #expect(again.pgn() == pgn)
        #expect(try again.node(id: 6) == chapter.node(id: 6))
        #expect(writeStudy(chapters: [chapter, again]) == pgn + "\n" + pgn)
    }

    @Test func edits() throws {
        let chapter = Chapter()
        let e4 = try chapter.addMove(after: chapter.root(), uci: "e2e4")
        #expect(try chapter.addMove(after: chapter.root(), uci: "e2e4") == e4)
        let d4 = try chapter.addMove(after: chapter.root(), uci: "d2d4")
        let e5 = try chapter.addMove(after: e4, uci: "e7e5")
        #expect(try chapter.node(id: chapter.root()).children == [e4, d4])
        #expect(try chapter.line(to: e5) == [e4, e5])

        try chapter.setComments(id: chapter.root(), comments: ["Mine."])
        try chapter.setAnnotations(id: e4, annotations: Annotations(moveMark: .good, positionSign: nil, others: [146]))
        try chapter.setShapes(id: e5, shapes: [DrawShape(brush: .red, from: 53, to: nil)])
        try chapter.setClock(id: e5, seconds: 9.4)
        try chapter.setTag(name: "ChapterName", value: "Mine")
        #expect(try chapter.node(id: e5).clock == 9.4)
        #expect(chapter.pgn() == """
            [ChapterName "Mine"]

            { Mine. } 1. e4! $146 (1. d4) 1... e5 { [%csl Rf7] [%clk 0:00:09.4] } *

            """)

        let fromFen = try Chapter.fromFen(fen: "4k3/8/8/8/8/8/8/4K2R w K - 0 1")
        #expect(try fromFen.node(id: fromFen.addMove(after: 0, uci: "e1h1")).played?.san == "O-O")
    }

    @Test func errors() throws {
        #expect(throws: ScacchiError.InvalidPgn(chapter: 2, at: "2. Ke3", reason: "illegal move")) {
            try readStudy(pgn: "1. e4 *\n\n1. e4 e5 2. Ke3 *")
        }
        #expect(throws: ScacchiError.InvalidPgn(chapter: 1, at: "", reason: "unsupported variant Atomic")) {
            try readStudy(pgn: "[Variant \"Atomic\"]\n\n1. e4 *")
        }
        let chapter = Chapter()
        #expect(throws: ScacchiError.UnknownNode(id: 7)) { try chapter.node(id: 7) }
        #expect(throws: ScacchiError.IllegalMove(uci: "e2e5")) { try chapter.addMove(after: 0, uci: "e2e5") }
        #expect(throws: ScacchiError.self) {
            try chapter.setAnnotations(id: 0, annotations: Annotations(moveMark: .good, positionSign: nil, others: []))
        }
        #expect(try readStudy(pgn: "").isEmpty)
    }
}
