//! Cases from chessops' `src/pgn.test.ts` and its `data/` files (see `chessops/README.md`),
//! adapted where Scacchi is stricter: it checks every move and rejects variants and null
//! moves.

use super::*;

fn file(name: &str) -> &'static str {
    match name {
        "kasparov-deep-blue-1997" => include_str!("chessops/kasparov-deep-blue-1997.pgn"),
        "headers-and-moves-on-the-same-line" => include_str!("chessops/headers-and-moves-on-the-same-line.pgn"),
        "pathological-headers" => include_str!("chessops/pathological-headers.pgn"),
        "leading-whitespace" => include_str!("chessops/leading-whitespace.pgn"),
        _ => unreachable!(),
    }
}

fn tag(chapter: &Chapter, name: &str) -> Option<String> {
    chapter.tags().into_iter().find(|t| t.name == name).map(|t| t.value)
}

fn mainline_sans(chapter: &Chapter) -> Vec<String> {
    sans(chapter, &chapter.mainline(chapter.root()).unwrap())
}

#[test]
fn kasparov_deep_blue_1997() {
    let chapters = read_study(file("kasparov-deep-blue-1997").into()).unwrap();
    assert_eq!(chapters.len(), 6);
    for chapter in &chapters {
        assert_eq!(tag(chapter, "Event").as_deref(), Some("IBM Man-Machine, New York USA"));
        let plies = tag(chapter, "PlyCount").unwrap().parse::<usize>().unwrap();
        assert_eq!(chapter.mainline(0).unwrap().len(), plies);
        assert_round_trip(chapter);
    }
    assert_eq!(&mainline_sans(&chapters[0])[..4], ["Nf3", "d5", "g3", "Bg4"]);
}

#[test]
fn headers_and_moves_on_the_same_line() {
    // Antichess, which Scacchi doesn't play: the first game stops the read.
    assert_eq!(
        read_study(file("headers-and-moves-on-the-same-line").into()).err(),
        Some(ScacchiError::InvalidPgn { chapter: 1, at: String::new(), reason: "unsupported variant Antichess".into() })
    );
    // The same layouts in standard chess.
    let standard = file("headers-and-moves-on-the-same-line").replace("Antichess", "Standard").replace("3.Qg4", "3.Qg4 Bxd2+");
    let chapters = read_study(standard).unwrap();
    assert_eq!(chapters.len(), 3);
    for chapter in &chapters {
        assert_eq!(mainline_sans(chapter), ["e3", "e6", "b4", "Bxb4", "Qg4", "Bxd2+"]);
        assert_round_trip(chapter);
    }
}

#[test]
fn pathological_headers() {
    let chapter = one(file("pathological-headers"));
    let expected = [("A", r#"b""#), ("B", r#"b""#), ("C", "A]]"), ("D", "A]]["), ("E", "\"A]][\""), ("F", r#""A]]["\"#), ("G", r#""]"#)];
    for (name, value) in expected {
        assert_eq!(tag(&chapter, name).as_deref(), Some(value), "{name}");
    }
    assert_round_trip(&chapter);
}

#[test]
fn leading_whitespace() {
    let chapters = read_study(file("leading-whitespace").into()).unwrap();
    assert_eq!(chapters.len(), 4);
    for chapter in &chapters {
        assert_eq!(mainline_sans(chapter), ["e4", "e5", "Nf3", "Nc6", "Bb5"]);
        assert_round_trip(chapter);
    }
}

#[test]
fn make_pgn() {
    let chapter = Chapter::new();
    let e4 = chapter.add_move(0, "e2e4".into()).unwrap();
    chapter.add_move(0, "e2e3".into()).unwrap();
    let e5 = chapter.add_move(e4, "e7e5".into()).unwrap();
    let e6 = chapter.add_move(e4, "e7e6".into()).unwrap();
    let nf3 = chapter.add_move(e6, "g1f3".into()).unwrap();
    chapter.add_move(e5, "c2c4".into()).unwrap();
    chapter.set_annotations(e4, Annotations { others: vec![7], ..Default::default() }).unwrap();
    chapter.set_comments(nf3, vec!["a comment".into()]).unwrap();
    // chessops writes `( 1. e3 )`; Scacchi, like Lichess's export, `(1. e3)`.
    assert_eq!(assert_round_trip(&chapter), "\n1. e4 $7 (1. e3) 1... e5 (1... e6 2. Nf3 { a comment }) 2. c4 *\n");
}

#[test]
fn extend_mainline() {
    let chapter = Chapter::new();
    let mut id = 0;
    // chessops' `Bg5` isn't legal there (d2 blocks the bishop): Bb5+ instead.
    for uci in ["e2e4", "d7d5", "a2a3", "h7h6", "f1b5"] {
        id = chapter.add_move(id, uci.into()).unwrap();
    }
    assert_eq!(chapter.pgn(), "\n1. e4 d5 2. a3 h6 3. Bb5+ *\n");
}

#[test]
fn parse_headers() {
    let pgn = [
        r#" [Event "event"]  "#,
        r#"[Black "black player"]"#,
        r#"[White " white  player   "]"#,
        r#"[Escaped "quote: \", backslashes: \\\\, trailing text"]"#,
        r#"[Multiple "on"] [the "same line"]"#,
    ]
    .join("\r\n");
    // chessops reads past an unterminated tag; pgn-reader stops, and so does Scacchi.
    assert_eq!(
        read_study(format!("{pgn}\r\n[Incomplete")).err(),
        Some(ScacchiError::InvalidPgn { chapter: 1, at: String::new(), reason: "unterminated tag".into() })
    );
    let chapter = one(&pgn);
    assert_eq!(tag(&chapter, "Event").as_deref(), Some("event"));
    assert_eq!(tag(&chapter, "Black").as_deref(), Some("black player"));
    assert_eq!(tag(&chapter, "White").as_deref(), Some(" white  player   "));
    assert_eq!(tag(&chapter, "Escaped").as_deref(), Some(r#"quote: ", backslashes: \\, trailing text"#));
    assert_eq!(tag(&chapter, "Multiple").as_deref(), Some("on"));
    assert_eq!(tag(&chapter, "the").as_deref(), Some("same line"));
    // chessops fills in the Seven Tag Roster (Result "*", Round "?"); Scacchi keeps what's there.
    assert_eq!(tag(&chapter, "Result"), None);
    assert_round_trip(&chapter);
}

#[test]
fn parse_pgn() {
    let chapter = one("1. e4 \ne5\nNf3 {foo\n  bar baz } 1-0");
    assert_eq!(assert_round_trip(&chapter), "[Result \"1-0\"]\n\n1. e4 e5 2. Nf3 { foo\n  bar baz } 1-0\n");
}

#[test]
fn transform_pgn() {
    // chessops reads the null move `--` and leaves it to the caller; Scacchi rejects it.
    assert_eq!(
        read_study("1. a4 ( 1. b4 b5 -- ) 1... a5".into()).err(),
        Some(ScacchiError::InvalidPgn { chapter: 1, at: "2. --".into(), reason: "illegal move".into() })
    );
    let chapter = one("1. a4 ( 1. b4 b5 ) 1... a5");
    let fen = |id| chapter.node(id).unwrap().fen;
    let root = chapter.node(0).unwrap();
    let (a4, b4) = (root.children[0], root.children[1]);
    assert_eq!(fen(a4), "rnbqkbnr/pppppppp/8/8/P7/8/1PPPPPPP/RNBQKBNR b KQkq - 0 1");
    assert_eq!(fen(chapter.node(a4).unwrap().children[0]), "rnbqkbnr/1ppppppp/8/p7/P7/8/1PPPPPPP/RNBQKBNR w KQkq - 0 2");
    assert_eq!(fen(b4), "rnbqkbnr/pppppppp/8/8/1P6/8/P1PPPPPP/RNBQKBNR b KQkq - 0 1");
    assert_eq!(fen(chapter.node(b4).unwrap().children[0]), "rnbqkbnr/p1pppppp/8/1p6/1P6/8/P1PPPPPP/RNBQKBNR w KQkq - 0 2");
}

#[test]
fn tricky_tokens() {
    // chessops reads `O-O-O !! 0-0-0# ??` from the start position without checking it.
    // Here White can castle long and Black short (the rook on d1 bars Black's long
    // castling); the zeros and the wrong `#` are read, and the SAN comes out right.
    let chapter = one("[FEN \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\"]\n\nO-O-O !! 0-0# ??");
    let main = chapter.mainline(0).unwrap();
    assert_eq!(sans(&chapter, &main), ["O-O-O", "O-O"]);
    assert_eq!(chapter.node(main[0]).unwrap().annotations.move_mark, Some(MoveMark::Brilliant));
    assert_eq!(chapter.node(main[1]).unwrap().annotations.move_mark, Some(MoveMark::Blunder));
}

#[test]
fn en_and_em_dashes() {
    // pgn-reader takes any dash in results and in castling written with zeros, but castling
    // with the letter O only with a hyphen: it skips `O—O` as a token it doesn't know,
    // without telling Scacchi, so the line just ends early. chessops reads both.
    let fen = "[FEN \"r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 14\"]\n\n";
    let chapter = one(&format!("{fen}14...0–0–0 15. O—O 1—0"));
    assert_eq!(mainline_sans(&chapter), ["O-O-O"]);
    assert_eq!(tag(&chapter, "Result").as_deref(), Some("1-0"));
    let chapter = one(&format!("{fen}14...0–0–0 15. 0—0 1—0"));
    assert_eq!(mainline_sans(&chapter), ["O-O-O", "O-O"]);
}

#[test]
fn parse_comment_cases() {
    let parsed = parse_comment("prefix [%emt 1:02:03.4]");
    assert_eq!((parsed.text.as_deref(), parsed.commands), (Some("prefix"), vec!["[%emt 1:02:03.4]".to_owned()]));

    let parsed = parse_comment("[%csl Ya1][%cal Ra1a1,Be1e2]commentary [%csl Gh8]");
    assert_eq!(parsed.text.as_deref(), Some("commentary"));
    assert_eq!(
        parsed.shapes,
        [
            DrawShape { brush: Brush::Yellow, from: 0, to: None },
            DrawShape { brush: Brush::Red, from: 0, to: None },
            DrawShape { brush: Brush::Blue, from: 4, to: Some(12) },
            DrawShape { brush: Brush::Green, from: 63, to: None },
        ]
    );

    for (comment, text, command) in [
        ("[%eval -0.42] suffix", "suffix", "[%eval -0.42]"),
        ("prefix [%eval .99,23]", "prefix", "[%eval .99,23]"),
        ("[%eval #-3] suffix", "suffix", "[%eval #-3]"),
    ] {
        let parsed = parse_comment(comment);
        assert_eq!((parsed.text.as_deref(), parsed.commands), (Some(text), vec![command.to_owned()]));
    }

    let parsed = parse_comment("[%csl Ga1]foo");
    assert_eq!(parsed.text.as_deref(), Some("foo"));
    assert_eq!(parsed.shapes, [DrawShape { brush: Brush::Green, from: 0, to: None }]);

    // chessops leaves the unknown `[%bar]` in the text; Scacchi keeps every other command
    // apart, raw.
    let parsed = parse_comment("foo [%bar] [%csl Ga1] [%cal Ra1h1,Gb1b8] [%clk 3:25:45]");
    assert_eq!(parsed.text.as_deref(), Some("foo"));
    assert_eq!(parsed.commands, ["[%bar]", "[%clk 3:25:45]"]);
    assert_eq!(parsed.shapes.len(), 3);
}

#[test]
fn make_comment() {
    let chapter = Chapter::new();
    let e4 = chapter.add_move(0, "e2e4".into()).unwrap();
    chapter.set_comments(e4, vec!["text".into()]).unwrap();
    chapter
        .set_shapes(e4, vec![
            DrawShape { brush: Brush::Yellow, from: 0, to: Some(0) },
            DrawShape { brush: Brush::Red, from: 0, to: Some(1) },
            DrawShape { brush: Brush::Red, from: 0, to: Some(2) },
        ])
        .unwrap();
    assert_eq!(assert_round_trip(&chapter), "\n1. e4 { text [%csl Ya1] [%cal Ra1b1,Ra1c1] } *\n");
}

#[test]
fn round_trip_comments() {
    for comment in [
        "[%csl[%eval 0.2] Ga1]",
        "[%c[%csl [%csl Ga1[%csl Ga1][%[%csl Ga1][%cal[%csl Ga1]Ra1]",
        "[%csl Ga1][%cal Ra1h1,Gb1b8] foo [%clk 3:ê5: [%eval 450752] [%evaÿTæ\u{92}ÿÿ\u{7f},7]",
    ] {
        assert_round_trip(&one(&format!("1. e4 {{{comment}}} *")));
    }
}
