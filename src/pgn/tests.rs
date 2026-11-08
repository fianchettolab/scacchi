use super::*;

/// A study chapter as Lichess exports it (hand-written: no real study's text).
const STUDY: &str = r#"[Event "Openings: Italian"]
[Site "https://lichess.org/study/abcd1234/efgh5678"]
[Result "*"]
[Variant "Standard"]
[ECO "C50"]
[Opening "Italian Game"]
[Annotator "https://lichess.org/@/someone"]
[ChapterName "Italian"]
[ChapterURL "https://lichess.org/study/abcd1234/efgh5678"]
[Orientation "black"]

{ The Italian. [%csl Ge4][%cal Ge2e4] } 1. e4 e5 2. Nf3 Nc6 3. Bc4 { [%cal Gc4f7,Rd8h4] } 3... Bc5 (3... Nf6 $5 { Two knights. } 4. Ng5 d5 5. exd5 Na5 (5... Nxd5?! 6. Nxf7!? $132) 6. Bb5+) 4. c3 $10 { [%clk 0:03:00] Quiet. [%eval 0.25] } *
"#;

fn sq(name: &str) -> u8 {
    parse_square(name).unwrap()
}

fn one(pgn: &str) -> Chapter {
    let mut chapters = read_study(pgn.into()).unwrap();
    assert_eq!(chapters.len(), 1);
    Arc::into_inner(chapters.pop().unwrap()).unwrap()
}

/// Everything in the tree but the ids, in book order, so trees built in different orders
/// compare.
fn dump(chapter: &Chapter) -> String {
    fn node(chapter: &Chapter, id: u32, out: &mut String, depth: usize) {
        let n = chapter.node(id).unwrap();
        let mut n_without_ids = n.clone();
        n_without_ids.id = 0;
        n_without_ids.parent = None;
        n_without_ids.children.clear();
        *out += &format!("{}{:?}\n", "  ".repeat(depth), n_without_ids);
        for child in n.children {
            node(chapter, child, out, depth + 1);
        }
    }
    let mut out = format!("{:?}\n", chapter.tags());
    node(chapter, chapter.root(), &mut out, 0);
    out
}

/// read → write → read gives the same tree, and writing again gives the same text.
fn assert_round_trip(chapter: &Chapter) -> String {
    let pgn = chapter.pgn();
    let again = one(&pgn);
    assert_eq!(dump(chapter), dump(&again), "\n{pgn}");
    assert_eq!(again.pgn(), pgn);
    pgn
}

fn sans(chapter: &Chapter, ids: &[u32]) -> Vec<String> {
    ids.iter().map(|&id| chapter.node(id).unwrap().played.unwrap().san).collect()
}

fn find(chapter: &Chapter, san: &str) -> Node {
    (1..)
        .map_while(|id| chapter.node(id).ok())
        .find(|n| n.played.as_ref().unwrap().san == san)
        .unwrap()
}

#[test]
fn reads_a_lichess_chapter() {
    let chapter = one(STUDY);
    assert_eq!(chapter.name().as_deref(), Some("Italian"));
    assert_eq!(chapter.orientation(), Side::Black);
    assert_eq!(chapter.tags().len(), 10);
    assert_eq!(chapter.tags()[3], PgnTag { name: "Variant".into(), value: "Standard".into() });

    let root = chapter.node(chapter.root()).unwrap();
    assert_eq!(root.comments, ["The Italian."]);
    assert_eq!(
        root.shapes,
        [
            DrawShape { brush: Brush::Green, from: sq("e4"), to: None },
            DrawShape { brush: Brush::Green, from: sq("e2"), to: Some(sq("e4")) },
        ]
    );
    assert_eq!((root.move_number, root.side), (1, Side::White));
    assert!(root.played.is_none());

    let main = chapter.mainline(chapter.root()).unwrap();
    assert_eq!(sans(&chapter, &main), ["e4", "e5", "Nf3", "Nc6", "Bc4", "Bc5", "c3"]);

    let bc4 = chapter.node(main[4]).unwrap();
    assert!(bc4.comments.is_empty());
    assert_eq!(bc4.shapes[1], DrawShape { brush: Brush::Red, from: sq("d8"), to: Some(sq("h4")) });

    let bc5 = chapter.node(main[5]).unwrap();
    assert_eq!((bc5.move_number, bc5.side), (3, Side::Black));
    assert_eq!(bc5.played.as_ref().unwrap().uci, "f8c5");
    assert_eq!(chapter.node(main[4]).unwrap().children.len(), 2);

    let c3 = chapter.node(main[6]).unwrap();
    assert_eq!(c3.annotations, Annotations { move_mark: None, position_sign: Some(PositionSign::Equal), others: vec![] });
    assert_eq!(c3.comments, ["Quiet."]);
    assert_eq!(c3.clock, Some(180.0));
    assert_eq!(c3.commands, ["[%eval 0.25]"]);
    assert_eq!(c3.fen, "r1bqk1nr/pppp1ppp/2n5/2b1p3/2B1P3/2P2N2/PP1P1PPP/RNBQK2R b KQkq - 0 4");

    let nf6 = find(&chapter, "Nf6");
    assert_eq!(nf6.annotations.move_mark, Some(MoveMark::Interesting));
    assert_eq!(nf6.comments, ["Two knights."]);
    let nxf7 = find(&chapter, "Nxf7");
    assert_eq!(
        nxf7.annotations,
        Annotations { move_mark: Some(MoveMark::Interesting), position_sign: None, others: vec![132] }
    );
    assert_eq!(sans(&chapter, &chapter.line(nxf7.id).unwrap()), ["e4", "e5", "Nf3", "Nc6", "Bc4", "Nf6", "Ng5", "d5", "exd5", "Nxd5", "Nxf7"]);
    assert_eq!(find(&chapter, "Nxd5").annotations.move_mark, Some(MoveMark::Dubious));
    assert!(find(&chapter, "Bb5+").played.is_some());
}

#[test]
fn writes_a_lichess_chapter() {
    let pgn = assert_round_trip(&one(STUDY));
    assert_eq!(
        pgn,
        r#"[Event "Openings: Italian"]
[Site "https://lichess.org/study/abcd1234/efgh5678"]
[Result "*"]
[Variant "Standard"]
[ECO "C50"]
[Opening "Italian Game"]
[Annotator "https://lichess.org/@/someone"]
[ChapterName "Italian"]
[ChapterURL "https://lichess.org/study/abcd1234/efgh5678"]
[Orientation "black"]

{ The Italian. [%csl Ge4] [%cal Ge2e4] } 1. e4 e5 2. Nf3 Nc6 3. Bc4
{ [%cal Gc4f7,Rd8h4] } 3... Bc5 (3... Nf6!? { Two knights. } 4. Ng5 d5 5. exd5
Na5 (5... Nxd5?! 6. Nxf7!? $132) 6. Bb5+) 4. c3 $10
{ Quiet. [%eval 0.25] [%clk 0:03:00] } *
"#
    );
    assert!(pgn.lines().all(|line| line.len() <= 80));
}

#[test]
fn numbers_moves_from_a_fen_with_black_to_move() {
    let chapter = one("[FEN \"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1\"]\n\n1... e5 2. Nf3 (2. f4 exf4) Nc6 *");
    let root = chapter.node(0).unwrap();
    assert_eq!((root.move_number, root.side), (1, Side::Black));
    let pgn = assert_round_trip(&chapter);
    assert!(pgn.starts_with("[SetUp \"1\"]\n[FEN \"rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1\"]\n\n"));
    assert!(pgn.ends_with("1... e5 2. Nf3 (2. f4 exf4) 2... Nc6 *\n"), "{pgn}");
}

#[test]
fn places_comments_as_chessops_does() {
    // A comment before a variation's first move starts it; one after `)` belongs to the
    // move the variation replaced; one after the opening `(` with no move is dropped.
    let chapter = one("{ Start. } 1. e4 { A } { B } (1. d4 { After d4. }) ({ Or the English. } 1. c4 () ) e5 (1... c5) { Note. } 2. Nf3 *");
    let root = chapter.node(0).unwrap();
    assert_eq!(root.comments, ["Start."]);
    let main = chapter.mainline(0).unwrap();
    assert_eq!(chapter.node(main[0]).unwrap().comments, ["A", "B"]);
    assert_eq!(find(&chapter, "d4").comments, ["After d4."]);
    let c4 = find(&chapter, "c4");
    assert_eq!(c4.starting_comments, ["Or the English."]);
    assert!(c4.comments.is_empty());
    assert_eq!(chapter.node(main[1]).unwrap().comments, ["Note."]);
    let pgn = assert_round_trip(&chapter);
    assert!(
        pgn.replace('\n', " ").contains(
            "1. e4 { A } { B } (1. d4 { After d4. }) ({ Or the English. } 1. c4) 1... e5 { Note. } (1... c5) 2. Nf3 *"
        ),
        "{pgn}"
    );
}

#[test]
fn shapes_and_commands() {
    let parsed = parse_comment("Look [%cal Ge2e4,Rd1h5] here [%csl Yf7] [%eval #-3,20] [%cal Xz9] [%evp 0.1,0.2] [%");
    assert_eq!(parsed.text.as_deref(), Some("Look here [%"));
    assert_eq!(parsed.shapes.len(), 3);
    assert_eq!(parsed.shapes[2], DrawShape { brush: Brush::Yellow, from: sq("f7"), to: None });
    assert_eq!(parsed.commands, ["[%eval #-3,20]", "[%cal Xz9]", "[%evp 0.1,0.2]"]);

    // A square in %cal is a circle, an arrow in %csl an arrow; order survives the writing.
    let chapter = one("1. e4 { [%cal Bd4] [%csl Ga1a2] [%cal Re5e6] } *");
    let e4 = chapter.node(1).unwrap();
    assert_eq!(e4.shapes[0], DrawShape { brush: Brush::Blue, from: sq("d4"), to: None });
    assert!(e4.comments.is_empty());
    let pgn = assert_round_trip(&chapter);
    assert!(pgn.contains("1. e4 { [%csl Bd4] [%cal Ga1a2,Re5e6] } *"), "{pgn}");
}

#[test]
fn clocks() {
    for (args, seconds) in [
        ("0:03:00", Some(180.0)),
        ("1:02:03", Some(3723.0)),
        ("0:00:09.4", Some(9.4)),
        (" 10:00:00 ", Some(36_000.0)),
        ("0:00:07.", Some(7.0)),
        ("3:ê5:", None),
        ("0:03", None),
        ("0:03:00:00", None),
        ("-0:03:00", None),
        ("0:03:00 later", None),
    ] {
        assert_eq!(parse_clock(args), seconds, "{args}");
    }
    for (seconds, written) in [
        (0.0, "[%clk 0:00:00]"),
        (180.0, "[%clk 0:03:00]"),
        (3723.0, "[%clk 1:02:03]"),
        (9.4, "[%clk 0:00:09.4]"),
        (59.9996, "[%clk 0:01:00]"),
        (0.125, "[%clk 0:00:00.125]"),
        (36_000.0, "[%clk 10:00:00]"),
    ] {
        assert_eq!(write_clock(seconds), written, "{seconds}");
    }

    // Read from any comment, written last in the move's last one, as chessops orders it.
    let chapter = one("1. e4 { [%clk 0:05:02] Fast. [%eval 0.3] } 1... e5 { [%clk 0:04:58] } { Slow. } 2. Nf3 { [%clk x] } *");
    let main = chapter.mainline(0).unwrap();
    let (e4, e5, nf3) = (chapter.node(main[0]).unwrap(), chapter.node(main[1]).unwrap(), chapter.node(main[2]).unwrap());
    assert_eq!((e4.clock, e4.comments.clone(), e4.commands.clone()), (Some(302.0), vec!["Fast.".to_owned()], vec!["[%eval 0.3]".to_owned()]));
    assert_eq!((e5.clock, e5.comments.clone()), (Some(298.0), vec!["Slow.".to_owned()]));
    assert_eq!((nf3.clock, nf3.commands.clone()), (None, vec!["[%clk x]".to_owned()]));
    let pgn = assert_round_trip(&chapter);
    assert!(pgn.replace('\n', " ").contains("1. e4 { Fast. [%eval 0.3] [%clk 0:05:02] } 1... e5 { Slow. [%clk 0:04:58] } 2. Nf3 { [%clk x] } *"), "{pgn}");

    // Set on a new chapter, a game as Lichess exports it; None removes it.
    let chapter = Chapter::new();
    let e4 = chapter.add_move(0, "e2e4".into()).unwrap();
    let e5 = chapter.add_move(e4, "e7e5".into()).unwrap();
    chapter.set_clock(e4, Some(182.0)).unwrap();
    chapter.set_clock(e5, Some(9.4)).unwrap();
    assert_eq!(assert_round_trip(&chapter), "\n1. e4 { [%clk 0:03:02] } 1... e5 { [%clk 0:00:09.4] } *\n");
    chapter.set_clock(e5, None).unwrap();
    assert_eq!(chapter.node(e5).unwrap().clock, None);
    assert_eq!(chapter.pgn(), "\n1. e4 { [%clk 0:03:02] } 1... e5 *\n");
    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(matches!(chapter.set_clock(e4, Some(bad)), Err(ScacchiError::InvalidEdit { .. })));
    }
    assert!(matches!(chapter.set_clock(99, Some(1.0)), Err(ScacchiError::UnknownNode { id: 99 })));
}

#[test]
fn several_chapters() {
    let study = format!("{STUDY}\n[Event \"Second\"]\n\n1. d4 d5 1-0\n\n[ChapterName \"Third\"]\n[Orientation \"white\"]\n\n*\n");
    let chapters = read_study(study).unwrap();
    assert_eq!(chapters.len(), 3);
    assert_eq!(chapters[1].name().as_deref(), Some("Second"));
    // The result from the movetext when there's no Result tag.
    assert_eq!(chapters[1].tags()[1], PgnTag { name: "Result".into(), value: "1-0".into() });
    assert_eq!(chapters[2].orientation(), Side::White);
    assert!(chapters[2].node(0).unwrap().children.is_empty());

    let written = write_study(chapters.clone());
    let again = read_study(written).unwrap();
    assert_eq!(again.len(), 3);
    for (a, b) in chapters.iter().zip(&again) {
        assert_eq!(dump(a), dump(b));
    }
    assert!(read_study(String::new()).unwrap().is_empty());
    assert!(read_study("\n  \n".into()).unwrap().is_empty());
}

#[test]
fn escapes_and_long_comments() {
    let chapter = one(r#"[Event "A \"quoted\" \\ name"]

1. e4 *"#);
    assert_eq!(chapter.name().as_deref(), Some(r#"A "quoted" \ name"#));
    let long = "word ".repeat(400);
    chapter.set_comments(1, vec![long.clone(), "  {braces}  ".into(), "   ".into()]).unwrap();
    assert_eq!(chapter.node(1).unwrap().comments, [long.trim(), "{braces"]);
    let pgn = assert_round_trip(&chapter);
    assert!(pgn.starts_with(r#"[Event "A \"quoted\" \\ name"]"#));
}

#[test]
fn edits() {
    let chapter = Chapter::new();
    let e4 = chapter.add_move(0, "e2e4".into()).unwrap();
    assert_eq!(chapter.add_move(0, "e2e4".into()).unwrap(), e4);
    let d4 = chapter.add_move(0, "d2d4".into()).unwrap();
    assert_eq!(chapter.node(0).unwrap().children, [e4, d4]);
    let e5 = chapter.add_move(e4, "e7e5".into()).unwrap();
    let ks = chapter.add_move(e5, "g1f3".into()).unwrap();
    assert_eq!(chapter.node(ks).unwrap().move_number, 2);
    assert_eq!(chapter.line(ks).unwrap(), [e4, e5, ks]);
    assert_eq!(chapter.line(0).unwrap(), Vec::<u32>::new());

    // Castling the king-takes-rook way finds the same move.
    let castle = Chapter::from_fen("4k3/8/8/8/8/8/8/4K2R w K - 0 1".into()).unwrap();
    let oo = castle.add_move(0, "e1g1".into()).unwrap();
    assert_eq!(castle.add_move(0, "e1h1".into()).unwrap(), oo);
    assert_eq!(castle.node(oo).unwrap().played.unwrap().san, "O-O");

    chapter
        .set_annotations(e4, Annotations { move_mark: None, position_sign: None, others: vec![146, 1, 16, 2] })
        .unwrap();
    assert_eq!(
        chapter.node(e4).unwrap().annotations,
        Annotations {
            move_mark: Some(MoveMark::Good),
            position_sign: Some(PositionSign::WhiteBetter),
            others: vec![146, 2],
        }
    );
    chapter
        .set_shapes(e5, vec![
            DrawShape { brush: Brush::Red, from: sq("f7"), to: Some(sq("f7")) },
            DrawShape { brush: Brush::Green, from: sq("g1"), to: Some(sq("f3")) },
        ])
        .unwrap();
    assert_eq!(chapter.node(e5).unwrap().shapes[0].to, None);
    chapter.set_comments(0, vec!["My analysis.".into()]).unwrap();
    chapter.set_tag("ChapterName".into(), Some("Mine".into())).unwrap();
    chapter.set_tag("Orientation".into(), Some("black".into())).unwrap();
    chapter.set_tag("Orientation".into(), None).unwrap();
    assert_eq!(chapter.name().as_deref(), Some("Mine"));
    assert_eq!(chapter.orientation(), Side::White);

    let pgn = assert_round_trip(&chapter);
    assert_eq!(
        pgn,
        "[ChapterName \"Mine\"]\n\n{ My analysis. } 1. e4! $16 $146 $2 (1. d4) 1... e5 { [%csl Rf7] [%cal Gg1f3] }\n2. Nf3 *\n"
    );

    let invalid = |r: Result<(), ScacchiError>| assert!(matches!(r, Err(ScacchiError::InvalidEdit { .. })), "{r:?}");
    invalid(chapter.set_annotations(0, Annotations::default()));
    invalid(chapter.set_annotations(e4, Annotations { others: vec![256], ..Default::default() }));
    invalid(chapter.set_shapes(e4, vec![DrawShape { brush: Brush::Blue, from: 64, to: None }]));
    invalid(chapter.set_tag("FEN".into(), Some("8/8/8/8/8/8/8/8 w - - 0 1".into())));
    invalid(chapter.set_tag("Variant".into(), Some("Chess960".into())));
    invalid(chapter.set_tag("Bad name".into(), Some("x".into())));
    assert_eq!(chapter.node(99), Err(ScacchiError::UnknownNode { id: 99 }));
    assert_eq!(chapter.add_move(99, "e2e4".into()), Err(ScacchiError::UnknownNode { id: 99 }));
    assert_eq!(chapter.add_move(e4, "e2e4".into()), Err(ScacchiError::IllegalMove { uci: "e2e4".into() }));
}

#[test]
fn errors() {
    let error = |pgn: &str| read_study(pgn.into()).err().unwrap();
    let invalid = |chapter: u32, at: &str, reason: &str| ScacchiError::InvalidPgn {
        chapter,
        at: at.into(),
        reason: reason.into(),
    };
    assert_eq!(error("1. e4 e5 2. Ke3 *"), invalid(1, "2. Ke3", "illegal move"));
    assert_eq!(error("1. e4 (1. d4 Nf6 2. Nxe5) *"), invalid(1, "2. Nxe5", "illegal move"));
    assert_eq!(error("1. e4 *\n\n1. e4 e5 2... Nf6 Nf3 *"), invalid(2, "2. Nf6", "illegal move"));
    assert_eq!(error("(1. d4) 1. e4 *"), invalid(1, "", "a variation with no move to replace"));
    assert_eq!(error("1. e4 ((1. d4)) *"), invalid(1, "", "a variation with no move to replace"));
    assert_eq!(error("1. e4 ) e5 *"), invalid(1, "", "a ) with no matching ("));
    assert_eq!(error("1. e4 (1. d4 *"), invalid(1, "", "a ( with no matching )"));
    assert_eq!(error("[Variant \"Chess960\"]\n\n1. e4 *"), invalid(1, "", "unsupported variant Chess960"));
    assert!(matches!(error("[FEN \"nonsense\"]\n\n*"), ScacchiError::InvalidPgn { chapter: 1, .. }));
}

mod chessops;
