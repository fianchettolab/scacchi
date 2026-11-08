//! Lichess study chapters in PGN: a tree of moves with variations, comments, annotations
//! (NAGs) and shapes (`[%csl]`, `[%cal]`, as chessops reads them), read with pgn-reader and
//! written back so that read → write → read gives the same tree.

use std::{
    io,
    ops::ControlFlow,
    sync::{Arc, Mutex, MutexGuard},
};

use pgn_reader::{Nag, Outcome, RawComment, RawTag, Reader, SanPlus, Skip, Visitor};
use shakmaty::{CastlingMode, Chess, EnPassantMode, Position, Square, fen::Fen};

use crate::{PlayedMove, ScacchiError, Side, uci};

/// A move's mark: `$1`–`$6`, written `!` `?` `!!` `??` `!?` `?!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum MoveMark {
    /// `!`, `$1`
    Good,
    /// `?`, `$2`
    Mistake,
    /// `!!`, `$3`
    Brilliant,
    /// `??`, `$4`
    Blunder,
    /// `!?`, `$5`
    Interesting,
    /// `?!`, `$6`
    Dubious,
}

impl MoveMark {
    const ALL: [MoveMark; 6] = [
        MoveMark::Good,
        MoveMark::Mistake,
        MoveMark::Brilliant,
        MoveMark::Blunder,
        MoveMark::Interesting,
        MoveMark::Dubious,
    ];

    fn nag(self) -> u8 {
        self as u8 + 1
    }

    fn symbol(self) -> &'static str {
        ["!", "?", "!!", "??", "!?", "?!"][self as usize]
    }
}

/// The position's assessment after a move, Lichess's eight signs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PositionSign {
    /// `=`, `$10`
    Equal,
    /// `∞`, `$13`
    Unclear,
    /// `⩲`, `$14`
    WhiteSlightlyBetter,
    /// `⩱`, `$15`
    BlackSlightlyBetter,
    /// `±`, `$16`
    WhiteBetter,
    /// `∓`, `$17`
    BlackBetter,
    /// `+−`, `$18`
    WhiteWinning,
    /// `−+`, `$19`
    BlackWinning,
}

impl PositionSign {
    const ALL: [PositionSign; 8] = [
        PositionSign::Equal,
        PositionSign::Unclear,
        PositionSign::WhiteSlightlyBetter,
        PositionSign::BlackSlightlyBetter,
        PositionSign::WhiteBetter,
        PositionSign::BlackBetter,
        PositionSign::WhiteWinning,
        PositionSign::BlackWinning,
    ];

    fn nag(self) -> u8 {
        [10, 13, 14, 15, 16, 17, 18, 19][self as usize]
    }
}

/// A move's annotations (NAGs).
#[derive(Debug, Clone, Default, PartialEq, Eq, uniffi::Record)]
pub struct Annotations {
    pub move_mark: Option<MoveMark>,
    pub position_sign: Option<PositionSign>,
    /// Every other NAG (0–255), in order: `$7` □, `$146` N, `$11`, … and a second mark or
    /// sign on the same move.
    pub others: Vec<u32>,
}

impl Annotations {
    /// Files a NAG the way the reader does: the first mark and the first sign get their
    /// fields, everything else goes to the others.
    fn add(&mut self, nag: u8) {
        let mark = MoveMark::ALL.into_iter().find(|m| m.nag() == nag);
        let sign = PositionSign::ALL.into_iter().find(|s| s.nag() == nag);
        if mark.is_some() && self.move_mark.is_none() {
            self.move_mark = mark;
        } else if sign.is_some() && self.position_sign.is_none() {
            self.position_sign = sign;
        } else {
            self.others.push(nag.into());
        }
    }

    fn is_empty(&self) -> bool {
        self.move_mark.is_none() && self.position_sign.is_none() && self.others.is_empty()
    }
}

/// Lichess's four brushes, written G, R, Y, B.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum Brush {
    Green,
    Red,
    Yellow,
    Blue,
}

impl Brush {
    const LETTERS: [(Brush, char); 4] =
        [(Brush::Green, 'G'), (Brush::Red, 'R'), (Brush::Yellow, 'Y'), (Brush::Blue, 'B')];

    fn letter(self) -> char {
        Self::LETTERS[self as usize].1
    }
}

/// An arrow, or a highlighted square when there's no `to`. Squares a1 = 0 … h8 = 63.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct DrawShape {
    pub brush: Brush,
    pub from: u8,
    pub to: Option<u8>,
}

/// A PGN tag pair, like `[White "Deep Blue"]`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PgnTag {
    pub name: String,
    pub value: String,
}

/// A node of a chapter's tree: the start position (the root) or a move.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Node {
    pub id: u32,
    /// None for the root.
    pub parent: Option<u32>,
    /// The main line first, then the variations, in order.
    pub children: Vec<u32>,
    /// The move into this node; None for the root.
    pub played: Option<PlayedMove>,
    /// The move's number and who played it, as written before it ("3." or "3..."). For the
    /// root, those of the first move from the start position.
    pub move_number: u32,
    pub side: Side,
    /// The position after the move (for the root, the start position).
    pub fen: String,
    /// Comments after the move, one for each `{…}`. The root's are the chapter's opening
    /// comment.
    pub comments: Vec<String>,
    /// Comments before the move, only at the first move of a variation.
    pub starting_comments: Vec<String>,
    pub annotations: Annotations,
    pub shapes: Vec<DrawShape>,
    /// The mover's time left after the move, in seconds, from `[%clk 0:03:00]`. The root's
    /// comes from the chapter's opening comment, as chessops reads it.
    pub clock: Option<f64>,
    /// Other `[%…]` commands (`[%eval 0.17]`), raw, written back as read.
    pub commands: Vec<String>,
}

struct NodeData {
    parent: Option<u32>,
    children: Vec<u32>,
    played: Option<(shakmaty::Move, PlayedMove)>,
    /// After the move.
    position: Chess,
    comments: Vec<String>,
    starting_comments: Vec<String>,
    annotations: Annotations,
    shapes: Vec<DrawShape>,
    clock: Option<f64>,
    commands: Vec<String>,
}

impl NodeData {
    fn new(parent: Option<u32>, played: Option<(shakmaty::Move, PlayedMove)>, position: Chess) -> Self {
        NodeData {
            parent,
            children: Vec::new(),
            played,
            position,
            comments: Vec::new(),
            starting_comments: Vec::new(),
            annotations: Annotations::default(),
            shapes: Vec::new(),
            clock: None,
            commands: Vec::new(),
        }
    }
}

struct Tree {
    /// Indexed by node id; the root is 0. Ids are never reused.
    nodes: Vec<NodeData>,
    /// Every tag but FEN and SetUp, which come from the root position.
    tags: Vec<PgnTag>,
}

impl Tree {
    fn new(position: Chess) -> Self {
        Tree { nodes: vec![NodeData::new(None, None, position)], tags: Vec::new() }
    }

    fn get(&self, id: u32) -> Result<&NodeData, ScacchiError> {
        self.nodes.get(id as usize).ok_or(ScacchiError::UnknownNode { id })
    }

    fn get_mut(&mut self, id: u32) -> Result<&mut NodeData, ScacchiError> {
        self.nodes.get_mut(id as usize).ok_or(ScacchiError::UnknownNode { id })
    }

    /// The child of `parent` reached by `m`, added as the last variation when it's new.
    fn child(&mut self, parent: u32, m: shakmaty::Move) -> u32 {
        let node = &self.nodes[parent as usize];
        if let Some(&existing) = node.children.iter().find(|&&c| {
            self.nodes[c as usize].played.as_ref().is_some_and(|(played, _)| *played == m)
        }) {
            return existing;
        }
        let before = node.position.clone();
        let played = PlayedMove {
            uci: uci(m),
            san: shakmaty::san::SanPlus::from_move(before.clone(), m).to_string(),
            is_capture: m.is_capture(),
        };
        let mut after = before;
        after.play_unchecked(m);
        let id = self.nodes.len() as u32;
        self.nodes.push(NodeData::new(Some(parent), Some((m, played)), after));
        self.nodes[parent as usize].children.push(id);
        id
    }

    /// The position the move into `id` was played from: its number and side.
    fn numbering(&self, id: u32) -> (u32, Side) {
        let node = &self.nodes[id as usize];
        let before = node.parent.map_or(&node.position, |p| &self.nodes[p as usize].position);
        (before.fullmoves().get(), before.turn().into())
    }

    fn node(&self, id: u32) -> Result<Node, ScacchiError> {
        let data = self.get(id)?;
        let (move_number, side) = self.numbering(id);
        Ok(Node {
            id,
            parent: data.parent,
            children: data.children.clone(),
            played: data.played.as_ref().map(|(_, p)| p.clone()),
            move_number,
            side,
            fen: fen(&data.position),
            comments: data.comments.clone(),
            starting_comments: data.starting_comments.clone(),
            annotations: data.annotations.clone(),
            shapes: data.shapes.clone(),
            clock: data.clock,
            commands: data.commands.clone(),
        })
    }

    fn tag(&self, name: &str) -> Option<&str> {
        self.tags.iter().find(|t| t.name == name).map(|t| t.value.as_str())
    }
}

fn fen(position: &Chess) -> String {
    Fen::from_position(position, EnPassantMode::Legal).to_string()
}

fn position_from_fen(fen: &str) -> Result<Chess, String> {
    Fen::from_ascii(fen.as_bytes())
        .map_err(|e| e.to_string())?
        .into_position(CastlingMode::Standard)
        .map_err(|e| e.to_string())
}

/// Variants that are standard chess, from the start or from a position.
fn is_standard_variant(variant: &str) -> bool {
    ["standard", "chess", "from position", "fromposition", "normal"]
        .contains(&variant.trim().to_ascii_lowercase().as_str())
}

/// A comment cleaned for writing: no `}`, which would end it, and no blank edges.
fn clean_comment(text: &str) -> Option<String> {
    let text = text.replace('}', "");
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

// MARK: - Comments

/// What a `{…}` holds: its text, its shapes, its clock and its other `[%…]` commands.
#[derive(Default)]
struct ParsedComment {
    text: Option<String>,
    shapes: Vec<DrawShape>,
    clock: Option<f64>,
    commands: Vec<String>,
}

/// The longest clock PGN can hold: chessops reads at most five digits of hours.
const MAX_CLOCK: f64 = 99_999.0 * 3600.0 + 59.0 * 60.0 + 59.999;

/// `[%clk 1:02:03]` or `[%clk 0:00:09.4]`'s time, in seconds, within chessops' limits: 1–5
/// digits of hours, 1–2 of minutes and of seconds, up to three decimals, and no more than
/// `MAX_CLOCK`, so whatever is read is written back the same.
fn parse_clock(args: &str) -> Option<f64> {
    let digits = |p: &str, max: usize| (1..=max).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_digit());
    let mut parts = args.trim().split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    let fraction_ok = fraction.len() <= 3 && fraction.bytes().all(|b| b.is_ascii_digit());
    if parts.next().is_some() || !digits(h, 5) || !digits(m, 2) || !digits(whole, 2) || !fraction_ok {
        return None;
    }
    let seconds = h.parse::<f64>().ok()? * 3600.0 + m.parse::<f64>().ok()? * 60.0 + s.parse::<f64>().ok()?;
    (seconds <= MAX_CLOCK).then_some(seconds)
}

/// The clock as chessops writes it: `H:MM:SS`, with up to three decimals when there are
/// any (`0:00:09.4`).
fn write_clock(seconds: f64) -> String {
    let millis = (seconds.max(0.0) * 1000.0).round() as u64;
    let (h, m, s, ms) = (millis / 3_600_000, millis / 60_000 % 60, millis / 1000 % 60, millis % 1000);
    let fraction = if ms == 0 { String::new() } else { format!(".{ms:03}").trim_end_matches('0').to_owned() };
    format!("[%clk {h}:{m:02}:{s:02}{fraction}]")
}

fn parse_square(s: &str) -> Option<u8> {
    s.parse::<Square>().ok().map(|sq| sq.to_u32() as u8)
}

/// `Ge4` or `Ge2e4`, as chessops reads them.
fn parse_shape(item: &str) -> Option<DrawShape> {
    let mut chars = item.chars();
    let letter = chars.next()?;
    let brush = Brush::LETTERS.into_iter().find(|&(_, l)| l == letter)?.0;
    let rest = chars.as_str();
    match rest.len() {
        2 => Some(DrawShape { brush, from: parse_square(rest)?, to: None }),
        4 if rest.is_ascii() => {
            let (from, to) = (parse_square(&rest[..2])?, parse_square(&rest[2..])?);
            Some(DrawShape { brush, from, to: (from != to).then_some(to) })
        }
        _ => None,
    }
}

fn parse_comment(comment: &str) -> ParsedComment {
    let mut parsed = ParsedComment::default();
    let mut text: Vec<&str> = Vec::new();
    let mut rest = comment;
    while let Some(start) = rest.find("[%") {
        let Some(len) = rest[start..].find(']') else { break };
        let command = &rest[start..start + len + 1];
        text.push(&rest[..start]);
        rest = &rest[start + len + 1..];

        let inner = &command[2..command.len() - 1];
        let (name, args) = inner.split_once(char::is_whitespace).unwrap_or((inner, ""));
        let shapes: Option<Vec<DrawShape>> = matches!(name, "csl" | "cal")
            .then(|| args.trim().split(',').map(|item| parse_shape(item.trim())).collect())
            .flatten();
        let clock = (name == "clk").then(|| parse_clock(args)).flatten();
        match (shapes, clock) {
            (Some(shapes), _) => parsed.shapes.extend(shapes),
            (_, Some(clock)) => parsed.clock = Some(clock),
            _ => parsed.commands.push(command.to_owned()),
        }
    }
    text.push(rest);
    let text: Vec<&str> = text.into_iter().map(str::trim).filter(|t| !t.is_empty()).collect();
    if !text.is_empty() {
        parsed.text = Some(text.join(" "));
    }
    parsed
}

/// Shapes as `[%csl …]` and `[%cal …]`, a new command each time the kind changes, so their
/// order survives.
fn write_shapes(shapes: &[DrawShape]) -> Vec<String> {
    let mut out = Vec::new();
    for group in shapes.chunk_by(|a, b| a.to.is_some() == b.to.is_some()) {
        let kind = if group[0].to.is_some() { "cal" } else { "csl" };
        let items: Vec<String> = group
            .iter()
            .map(|s| {
                let square = |sq: u8| Square::new(sq.into()).to_string();
                format!("{}{}{}", s.brush.letter(), square(s.from), s.to.map(square).unwrap_or_default())
            })
            .collect();
        out.push(format!("[%{kind} {}]", items.join(",")));
    }
    out
}

// MARK: - Reading

struct Frame {
    /// The last node of this line: where the next move goes.
    node: u32,
    /// Whether the line has a move yet. A variation starts at the parent of the move it
    /// replaces, so its first comments wait for its first move.
    started: bool,
    pending: ParsedComment,
    pending_texts: Vec<String>,
}

struct Movetext {
    tree: Tree,
    lines: Vec<Frame>,
    outcome: Option<String>,
}

struct TagsSoFar {
    tags: Vec<PgnTag>,
    fen: Option<String>,
}

struct ChapterReader {
    /// 1-based, for errors.
    chapter: u32,
    comment: Vec<u8>,
}

impl ChapterReader {
    fn error(&self, at: String, reason: impl Into<String>) -> ScacchiError {
        ScacchiError::InvalidPgn { chapter: self.chapter, at, reason: reason.into() }
    }
}

type Output = Result<Tree, ScacchiError>;

impl Visitor for ChapterReader {
    type Tags = TagsSoFar;
    type Movetext = Movetext;
    type Output = Output;

    fn begin_tags(&mut self) -> ControlFlow<Output, TagsSoFar> {
        ControlFlow::Continue(TagsSoFar { tags: Vec::new(), fen: None })
    }

    fn tag(&mut self, tags: &mut TagsSoFar, name: &[u8], value: RawTag<'_>) -> ControlFlow<Output> {
        let name = String::from_utf8_lossy(name).into_owned();
        let value = unescape_tag(value.as_bytes());
        match name.as_str() {
            "FEN" => tags.fen = Some(value),
            "SetUp" => {}
            "Variant" if !is_standard_variant(&value) => {
                return ControlFlow::Break(Err(self.error(String::new(), format!("unsupported variant {value}"))));
            }
            _ => tags.tags.push(PgnTag { name, value }),
        }
        ControlFlow::Continue(())
    }

    fn begin_movetext(&mut self, tags: TagsSoFar) -> ControlFlow<Output, Movetext> {
        let position = match tags.fen {
            None => Chess::default(),
            Some(fen) => match position_from_fen(&fen) {
                Ok(position) => position,
                Err(reason) => {
                    return ControlFlow::Break(Err(self.error(String::new(), format!("invalid FEN {fen}: {reason}"))));
                }
            },
        };
        let mut tree = Tree::new(position);
        tree.tags = tags.tags;
        let root = Frame { node: 0, started: true, pending: ParsedComment::default(), pending_texts: Vec::new() };
        ControlFlow::Continue(Movetext { tree, lines: vec![root], outcome: None })
    }

    fn san(&mut self, movetext: &mut Movetext, san_plus: SanPlus) -> ControlFlow<Output> {
        let tree = &mut movetext.tree;
        let line = movetext.lines.last_mut().unwrap();
        let parent = line.node;
        let before = &tree.nodes[parent as usize].position;
        let Ok(m) = san_plus.san.to_move(before) else {
            let number = before.fullmoves();
            let dots = if before.turn().is_white() { "." } else { "..." };
            return ControlFlow::Break(Err(self.error(format!("{number}{dots} {san_plus}"), "illegal move")));
        };
        let id = tree.child(parent, m);
        let node = &mut tree.nodes[id as usize];
        node.starting_comments.append(&mut line.pending_texts);
        let pending = std::mem::take(&mut line.pending);
        node.shapes.extend(pending.shapes);
        node.clock = pending.clock.or(node.clock);
        node.commands.extend(pending.commands);
        line.node = id;
        line.started = true;
        ControlFlow::Continue(())
    }

    fn nag(&mut self, movetext: &mut Movetext, nag: Nag) -> ControlFlow<Output> {
        // A NAG before a line's first move has no move to go to: dropped.
        let line = movetext.lines.last().unwrap();
        if line.started && line.node != 0 {
            movetext.tree.nodes[line.node as usize].annotations.add(nag.0);
        }
        ControlFlow::Continue(())
    }

    fn partial_comment(&mut self, _: &mut Movetext, comment: RawComment<'_>) -> ControlFlow<Output> {
        self.comment.extend_from_slice(comment.as_bytes());
        ControlFlow::Continue(())
    }

    fn comment(&mut self, movetext: &mut Movetext, comment: RawComment<'_>) -> ControlFlow<Output> {
        self.comment.extend_from_slice(comment.as_bytes());
        let parsed = parse_comment(&String::from_utf8_lossy(&std::mem::take(&mut self.comment)));
        let line = movetext.lines.last_mut().unwrap();
        if line.started {
            let node = &mut movetext.tree.nodes[line.node as usize];
            node.comments.extend(parsed.text);
            node.shapes.extend(parsed.shapes);
            node.clock = parsed.clock.or(node.clock);
            node.commands.extend(parsed.commands);
        } else {
            line.pending_texts.extend(parsed.text);
            line.pending.shapes.extend(parsed.shapes);
            line.pending.clock = parsed.clock.or(line.pending.clock);
            line.pending.commands.extend(parsed.commands);
        }
        ControlFlow::Continue(())
    }

    fn begin_variation(&mut self, movetext: &mut Movetext) -> ControlFlow<Output, Skip> {
        let line = movetext.lines.last().unwrap();
        let replaced = movetext.tree.nodes[line.node as usize].parent;
        let (true, Some(parent)) = (line.started, replaced) else {
            return ControlFlow::Break(Err(self.error(String::new(), "a variation with no move to replace")));
        };
        movetext.lines.push(Frame {
            node: parent,
            started: false,
            pending: ParsedComment::default(),
            pending_texts: Vec::new(),
        });
        ControlFlow::Continue(Skip(false))
    }

    fn end_variation(&mut self, movetext: &mut Movetext) -> ControlFlow<Output> {
        if movetext.lines.len() == 1 {
            return ControlFlow::Break(Err(self.error(String::new(), "a ) with no matching (")));
        }
        // An empty variation's comments have no move to go to: dropped.
        movetext.lines.pop();
        ControlFlow::Continue(())
    }

    fn outcome(&mut self, movetext: &mut Movetext, outcome: Outcome) -> ControlFlow<Output> {
        movetext.outcome = Some(outcome.to_string());
        ControlFlow::Continue(())
    }

    fn end_game(&mut self, movetext: Movetext) -> Output {
        let mut tree = movetext.tree;
        if movetext.lines.len() > 1 {
            return Err(self.error(String::new(), "a ( with no matching )"));
        }
        if let Some(outcome) = movetext.outcome.filter(|o| o != "*")
            && tree.tag("Result").is_none()
        {
            tree.tags.push(PgnTag { name: "Result".into(), value: outcome });
        }
        Ok(tree)
    }
}

fn read(pgn: &str) -> Result<Vec<Tree>, ScacchiError> {
    let mut reader = Reader::build(io::Cursor::new(pgn.as_bytes()))
        .set_supported_tag_line_length(1 << 16)
        .set_supported_comment_length(1 << 16)
        .finish();
    let mut trees = Vec::new();
    loop {
        let mut visitor = ChapterReader { chapter: trees.len() as u32 + 1, comment: Vec::new() };
        // From memory, an I/O error is bad syntax pgn-reader can't get past, like an
        // unterminated tag.
        let game = reader.read_game(&mut visitor).map_err(|e| visitor.error(String::new(), e.to_string()))?;
        match game {
            None => return Ok(trees),
            Some(tree) => trees.push(tree?),
        }
    }
}

// MARK: - Writing

struct Writer<'a> {
    tree: &'a Tree,
    tokens: Vec<String>,
}

impl Writer<'_> {
    fn comment(&mut self, text: &str) {
        self.tokens.push(format!("{{ {text} }}"));
    }

    /// The node's comments, its shapes, commands and clock appended to the last one (or in
    /// a comment of their own), in chessops' order. Returns whether anything was written.
    fn comments(&mut self, node: &NodeData) -> bool {
        let mut extras = write_shapes(&node.shapes);
        extras.extend(node.commands.iter().cloned());
        extras.extend(node.clock.map(write_clock));
        let mut texts: Vec<String> = node.comments.iter().filter_map(|c| clean_comment(c)).collect();
        if !extras.is_empty() {
            match texts.last_mut() {
                Some(last) => *last = format!("{last} {}", extras.join(" ")),
                None => texts.push(extras.join(" ")),
            }
        }
        for text in &texts {
            self.comment(text);
        }
        !texts.is_empty()
    }

    /// Writes the move into `id`; returns whether the next move needs its number.
    fn write_move(&mut self, id: u32, mut force_number: bool) -> bool {
        let node = &self.tree.nodes[id as usize];
        for text in node.starting_comments.iter().filter_map(|c| clean_comment(c)) {
            self.comment(&text);
            force_number = true;
        }
        let (number, side) = self.tree.numbering(id);
        match side {
            Side::White => self.tokens.push(format!("{number}.")),
            Side::Black if force_number => self.tokens.push(format!("{number}...")),
            Side::Black => {}
        }
        // A move mark goes on the move, as Lichess writes it (`Nf6!?`).
        let a = &node.annotations;
        let san = &node.played.as_ref().unwrap().1.san;
        self.tokens.push(format!("{san}{}", a.move_mark.map_or("", MoveMark::symbol)));
        let mut forced = !a.is_empty();
        let nags = a.position_sign.map(PositionSign::nag).map(u32::from).into_iter().chain(a.others.iter().copied());
        self.tokens.extend(nags.map(|n| format!("${n}")));
        forced |= self.comments(node);
        forced
    }

    /// The line from `parent` on: its main move, that move's alternatives in brackets, then
    /// on from the main move.
    fn write_line(&mut self, mut parent: u32, mut force_number: bool) {
        loop {
            let children = &self.tree.nodes[parent as usize].children;
            let Some((&main, variations)) = children.split_first() else { return };
            force_number = self.write_move(main, force_number);
            for &variation in variations {
                self.tokens.push("(".into());
                let force = self.write_move(variation, true);
                self.write_line(variation, force);
                self.tokens.push(")".into());
                force_number = true;
            }
            parent = main;
        }
    }
}

/// `\"` and `\\` in a tag value. Not pgn-reader's `RawTag::decode`, which in 0.29.1 takes
/// an escaped backslash for the start of another escape (`\\\\` gives one backslash).
fn unescape_tag(raw: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let mut bytes = raw.iter().copied();
    while let Some(b) = bytes.next() {
        match (b, bytes.clone().next()) {
            (b'\\', Some(next @ (b'\\' | b'"'))) => {
                out.push(next);
                bytes.next();
            }
            _ => out.push(b),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape_tag(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace(['\n', '\r'], " ")
}

fn write(tree: &Tree) -> String {
    let mut out = String::new();
    for tag in &tree.tags {
        out += &format!("[{} \"{}\"]\n", tag.name, escape_tag(&tag.value));
    }
    let root = &tree.nodes[0];
    let start = fen(&root.position);
    if start != fen(&Chess::default()) {
        out += &format!("[SetUp \"1\"]\n[FEN \"{start}\"]\n");
    }
    out.push('\n');

    let mut writer = Writer { tree, tokens: Vec::new() };
    writer.comments(root);
    writer.write_line(0, true);
    let result = tree.tag("Result").filter(|r| ["1-0", "0-1", "1/2-1/2"].contains(r)).unwrap_or("*");
    writer.tokens.push(result.to_owned());

    // 80-column lines, a token never split.
    let mut line = String::new();
    for token in writer.tokens {
        if !line.is_empty() && line.len() + 1 + token.len() > 80 {
            out += &line;
            out.push('\n');
            line.clear();
        }
        // Brackets hug what they hold: `(3... Nf6)`.
        if !line.is_empty() && !line.ends_with('(') && token != ")" {
            line.push(' ');
        }
        line += &token;
    }
    out += &line;
    out.push('\n');
    out
}

// MARK: - API

/// A study chapter: tags and a tree of moves from a start position, with variations,
/// comments, annotations and shapes. Thread-safe: every method takes the lock.
#[derive(uniffi::Object)]
pub struct Chapter {
    tree: Mutex<Tree>,
}

impl Chapter {
    fn from_tree(tree: Tree) -> Self {
        Chapter { tree: Mutex::new(tree) }
    }

    fn lock(&self) -> MutexGuard<'_, Tree> {
        self.tree.lock().unwrap()
    }
}

#[uniffi::export]
impl Chapter {
    /// An empty chapter from the standard start position.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Chapter::from_tree(Tree::new(Chess::default()))
    }

    /// An empty chapter from a FEN.
    #[uniffi::constructor]
    pub fn from_fen(fen: String) -> Result<Self, ScacchiError> {
        let position = position_from_fen(&fen).map_err(|reason| ScacchiError::InvalidFen { fen, reason })?;
        Ok(Chapter::from_tree(Tree::new(position)))
    }

    /// The root's id: the start position.
    pub fn root(&self) -> u32 {
        0
    }

    pub fn node(&self, id: u32) -> Result<Node, ScacchiError> {
        self.lock().node(id)
    }

    /// The main line on from `from`: its first child, that one's first child, and so on
    /// (`from` itself not included).
    pub fn mainline(&self, from: u32) -> Result<Vec<u32>, ScacchiError> {
        let tree = self.lock();
        let mut id = from;
        tree.get(id)?;
        let mut line = Vec::new();
        while let Some(&next) = tree.nodes[id as usize].children.first() {
            line.push(next);
            id = next;
        }
        Ok(line)
    }

    /// The moves from the start to `to`, `to` included (empty for the root).
    pub fn line(&self, to: u32) -> Result<Vec<u32>, ScacchiError> {
        let tree = self.lock();
        let mut line = Vec::new();
        let mut id = Some(to);
        tree.get(to)?;
        while let Some(current) = id.filter(|&i| i != 0) {
            line.push(current);
            id = tree.nodes[current as usize].parent;
        }
        line.reverse();
        Ok(line)
    }

    /// Plays a UCI move after node `after`: the existing child when that move is already
    /// there, otherwise a new one as its last variation (or its main line, when it has no
    /// children). Returns the child's id.
    pub fn add_move(&self, after: u32, uci: String) -> Result<u32, ScacchiError> {
        let mut tree = self.lock();
        let illegal = || ScacchiError::IllegalMove { uci: uci.clone() };
        let m = uci
            .parse::<shakmaty::uci::UciMove>()
            .map_err(|_| illegal())?
            .to_move(&tree.get(after)?.position)
            .map_err(|_| illegal())?;
        Ok(tree.child(after, m))
    }

    /// The comments after the move (for the root, the chapter's opening comment). Each is
    /// trimmed, `}` is dropped (PGN can't hold it) and empty ones go.
    pub fn set_comments(&self, id: u32, comments: Vec<String>) -> Result<(), ScacchiError> {
        self.lock().get_mut(id)?.comments = comments.iter().filter_map(|c| clean_comment(c)).collect();
        Ok(())
    }

    /// Filed the way the reader files them: a mark or sign among the others moves to its
    /// field when that's empty. Only moves have annotations, so not the root.
    pub fn set_annotations(&self, id: u32, annotations: Annotations) -> Result<(), ScacchiError> {
        let mut tree = self.lock();
        let node = tree.get_mut(id)?;
        if node.parent.is_none() {
            return Err(ScacchiError::InvalidEdit { reason: "the start position has no move to annotate".into() });
        }
        let mut filed =
            Annotations { move_mark: annotations.move_mark, position_sign: annotations.position_sign, others: Vec::new() };
        for nag in annotations.others {
            let nag = u8::try_from(nag)
                .map_err(|_| ScacchiError::InvalidEdit { reason: format!("NAG {nag} is over 255") })?;
            filed.add(nag);
        }
        node.annotations = filed;
        Ok(())
    }

    /// The arrows and highlighted squares on the position after the move (for the root, on
    /// the start position). An arrow to its own square is a highlighted square.
    pub fn set_shapes(&self, id: u32, shapes: Vec<DrawShape>) -> Result<(), ScacchiError> {
        let mut tree = self.lock();
        let node = tree.get_mut(id)?;
        if let Some(bad) = shapes.iter().flat_map(|s| [Some(s.from), s.to]).flatten().find(|&sq| sq > 63) {
            return Err(ScacchiError::InvalidEdit { reason: format!("square {bad} is off the board") });
        }
        node.shapes = shapes.into_iter().map(|s| DrawShape { to: s.to.filter(|&to| to != s.from), ..s }).collect();
        Ok(())
    }

    /// The mover's time left after the move, in seconds, written as `[%clk]`; None removes
    /// it. Kept to the millisecond, as it's written, so round it first for whole seconds.
    /// Up to 99999:59:59.999, the most PGN's clock holds.
    pub fn set_clock(&self, id: u32, seconds: Option<f64>) -> Result<(), ScacchiError> {
        if let Some(bad) = seconds.filter(|s| !s.is_finite() || *s < 0.0 || *s > MAX_CLOCK) {
            return Err(ScacchiError::InvalidEdit { reason: format!("clock {bad} isn't a time left") });
        }
        self.lock().get_mut(id)?.clock = seconds.map(|s| (s * 1000.0).round() / 1000.0);
        Ok(())
    }

    /// Every tag in order, FEN and SetUp aside (they come from the start position).
    pub fn tags(&self) -> Vec<PgnTag> {
        self.lock().tags.clone()
    }

    /// Sets a tag, replacing one of the same name in place or adding it at the end; None
    /// removes it. FEN and SetUp can't be set: they come from the start position.
    pub fn set_tag(&self, name: String, value: Option<String>) -> Result<(), ScacchiError> {
        let invalid = |reason: String| Err(ScacchiError::InvalidEdit { reason });
        if name == "FEN" || name == "SetUp" {
            return invalid(format!("{name} comes from the start position"));
        }
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return invalid(format!("invalid tag name {name:?}"));
        }
        if name == "Variant" && value.as_deref().is_some_and(|v| !is_standard_variant(v)) {
            return invalid(format!("unsupported variant {}", value.unwrap()));
        }
        let mut tree = self.lock();
        let index = tree.tags.iter().position(|t| t.name == name);
        match (index, value) {
            (Some(i), Some(value)) => tree.tags[i].value = value,
            (None, Some(value)) => tree.tags.push(PgnTag { name, value }),
            (Some(i), None) => {
                tree.tags.remove(i);
            }
            (None, None) => {}
        }
        Ok(())
    }

    /// The chapter's name: its ChapterName tag, otherwise its Event.
    pub fn name(&self) -> Option<String> {
        let tree = self.lock();
        tree.tag("ChapterName").or_else(|| tree.tag("Event")).map(str::to_owned)
    }

    /// Which side the board shows at the bottom: the Orientation tag, White by default.
    pub fn orientation(&self) -> Side {
        match self.lock().tag("Orientation").map(|o| o.trim().to_ascii_lowercase()) {
            Some(o) if o == "black" => Side::Black,
            _ => Side::White,
        }
    }

    /// The chapter in PGN.
    pub fn pgn(&self) -> String {
        write(&self.lock())
    }
}

impl Default for Chapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Every chapter in a PGN (a study's export: one game per chapter), in order. Fails on the
/// first chapter with an illegal move, an unmatched bracket, an invalid FEN or a variant
/// other than standard chess.
#[uniffi::export]
pub fn read_study(pgn: String) -> Result<Vec<Arc<Chapter>>, ScacchiError> {
    Ok(read(&pgn)?.into_iter().map(|tree| Arc::new(Chapter::from_tree(tree))).collect())
}

/// The chapters' PGNs, one after the other.
#[uniffi::export]
pub fn write_study(chapters: Vec<Arc<Chapter>>) -> String {
    chapters.iter().map(|c| c.pgn()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests;
