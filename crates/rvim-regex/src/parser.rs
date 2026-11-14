//! Vim-compatible pattern parser.
//!
//! The parser is a single-pass recursive-descent parser over bytes.  Magic
//! modes can change at any point via \v, \m, \M and \V, so every token check
//! consults the current mode rather than pre-tokenising the pattern.

use crate::ast::{Anchor, Ast, LookKind};
use crate::charclass::{self, ByteSet, CharClass};
use crate::error::RegexError;
use crate::Magic;

/// Result of parsing a pattern.
pub(crate) struct Parsed {
    pub(crate) ast: Ast,
    pub(crate) groups: usize,
    /// Capture slots for `\z(` groups, indexed by syntax-group number - 1.
    pub(crate) z_groups: Vec<usize>,
    /// Result of \c / \C.  A \c anywhere wins over \C, matching Vim.
    pub(crate) force_ignorecase: Option<bool>,
}

struct Parser<'a> {
    pattern: &'a [u8],
    pos: usize,
    magic: Magic,
    groups: usize,
    z_groups: Vec<usize>,
    at_branch_start: bool,
    saw_ignorecase: bool,
    saw_matchcase: bool,
}

#[derive(Debug, Clone, Copy)]
struct QuantToken {
    kind: QuantKind,
    offset: usize,
    len: usize,
}

#[derive(Debug, Clone, Copy)]
enum QuantKind {
    Star,
    Plus,
    Optional,
    Interval,
}

#[derive(Debug, Clone, Copy)]
enum LookTokenKind {
    Ahead,
    NotAhead,
    Behind(Option<usize>),
    NotBehind(Option<usize>),
    Atomic,
}

#[derive(Debug, Clone, Copy)]
struct LookToken {
    kind: LookTokenKind,
    offset: usize,
    end: usize,
}

#[derive(Debug, Clone, Copy)]
enum ClassItem {
    Byte(u8),
    Set(ByteSet),
}

/// Parse a pattern.
pub(crate) fn parse(pattern: &[u8], magic: Magic) -> Result<Parsed, RegexError> {
    Parser {
        pattern,
        pos: 0,
        magic,
        groups: 0,
        z_groups: Vec::new(),
        at_branch_start: true,
        saw_ignorecase: false,
        saw_matchcase: false,
    }
    .parse_all()
}

impl<'a> Parser<'a> {
    fn parse_all(mut self) -> Result<Parsed, RegexError> {
        let ast = self.parse_alt()?;
        if self.pos < self.pattern.len() {
            return Err(self.err("E55: Unmatched \\)", self.pos));
        }
        let force_ignorecase = if self.saw_ignorecase {
            Some(true)
        } else if self.saw_matchcase {
            Some(false)
        } else {
            None
        };
        Ok(Parsed {
            ast,
            groups: self.groups,
            z_groups: self.z_groups,
            force_ignorecase,
        })
    }

    fn parse_alt(&mut self) -> Result<Ast, RegexError> {
        let mut branches = Vec::new();
        loop {
            branches.push(self.parse_branch()?);
            if !self.eat_alt() {
                break;
            }
            self.at_branch_start = true;
        }
        match branches.len() {
            0 => Ok(Ast::Empty),
            1 => Ok(branches.remove(0)),
            _ => Ok(Ast::Alt(branches)),
        }
    }

    /// A branch is one or more concats separated by `\&`.  It matches the
    /// last concat, but only if all preceding concats also match at the same
    /// position (identical to wrapping them in `\@=`).
    fn parse_branch(&mut self) -> Result<Ast, RegexError> {
        let mut concats = vec![self.parse_concat()?];
        while self.eat_conjunction() {
            self.at_branch_start = true;
            concats.push(self.parse_concat()?);
        }
        if concats.len() == 1 {
            return Ok(concats.remove(0));
        }
        let last = concats.pop().expect("at least two concats");
        let mut pieces: Vec<Ast> = concats
            .into_iter()
            .map(|node| Ast::Look {
                kind: LookKind::Ahead,
                node: Box::new(node),
                limit: None,
            })
            .collect();
        pieces.push(last);
        Ok(Ast::Concat(pieces))
    }

    fn parse_concat(&mut self) -> Result<Ast, RegexError> {
        let mut pieces = Vec::new();
        let mut star_literal_allowed = true;
        loop {
            if self.at_concat_end() {
                break;
            }
            let piece = self.parse_piece(star_literal_allowed)?;
            self.update_branch_start(&piece);
            star_literal_allowed = match &piece {
                Ast::Empty => star_literal_allowed,
                Ast::Anchor(Anchor::LineStart) => true,
                _ => false,
            };
            pieces.push(piece);
        }
        match pieces.len() {
            0 => Ok(Ast::Empty),
            1 => Ok(pieces.remove(0)),
            _ => Ok(Ast::Concat(pieces)),
        }
    }

    fn update_branch_start(&mut self, piece: &Ast) {
        self.at_branch_start = match piece {
            Ast::Empty => self.at_branch_start,
            Ast::Literal(b'\n') => true,
            _ => false,
        };
    }

    fn parse_piece(&mut self, star_literal_allowed: bool) -> Result<Ast, RegexError> {
        let atom = self.parse_atom(star_literal_allowed)?;
        if let Some(token) = self.peek_lookaround()? {
            self.pos = token.end;
            if self.multi_follows()? {
                return Err(self.err("E61: Nested multi after look-around", token.offset));
            }
            let node = Box::new(atom);
            return Ok(match token.kind {
                LookTokenKind::Ahead => Ast::Look {
                    kind: LookKind::Ahead,
                    node,
                    limit: None,
                },
                LookTokenKind::NotAhead => Ast::Look {
                    kind: LookKind::NotAhead,
                    node,
                    limit: None,
                },
                LookTokenKind::Behind(limit) => Ast::Look {
                    kind: LookKind::Behind,
                    node,
                    limit,
                },
                LookTokenKind::NotBehind(limit) => Ast::Look {
                    kind: LookKind::NotBehind,
                    node,
                    limit,
                },
                LookTokenKind::Atomic => Ast::Atomic(node),
            });
        }
        let star_after_line_start =
            matches!(atom, Ast::Anchor(Anchor::LineStart)) && self.peek() == Some(b'*');
        if star_after_line_start {
            return Ok(atom);
        }
        if let Some(token) = self.peek_quantifier() {
            if matches!(atom, Ast::SetStart | Ast::SetEnd) {
                return Err(self.err("E888: cannot repeat \\zs or \\ze", token.offset));
            }
            let offset = token.offset;
            let (min, max, greedy) = self.consume_quantifier(token)?;
            if self.multi_follows()? {
                return Err(self.err("E61: Nested multi", offset));
            }
            return Ok(Ast::Repeat {
                node: Box::new(atom),
                min,
                max,
                greedy,
            });
        }
        Ok(atom)
    }

    fn parse_atom(&mut self, star_literal_allowed: bool) -> Result<Ast, RegexError> {
        let offset = self.pos;
        let byte = match self.peek() {
            Some(byte) => byte,
            None => return Ok(Ast::Empty),
        };
        if byte == b'\\' {
            return self.parse_escape();
        }
        match self.magic {
            Magic::VeryMagic => match byte {
                b'.' => {
                    self.pos += 1;
                    Ok(Ast::Any)
                }
                b'[' => self.parse_class(false),
                b'^' => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::LineStart))
                }
                b'$' => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::LineEnd))
                }
                b'<' => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::WordStart))
                }
                b'>' => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::WordEnd))
                }
                b'(' => {
                    self.pos += 1;
                    self.parse_group(true, offset)
                }
                b'%' => self.parse_percent_very_magic(offset),
                b'@' => match self.peek_lookaround()? {
                    Some(_) => Err(self.err("E64: @ follows nothing", offset)),
                    None => Err(self.err("E59: Invalid character after @", offset)),
                },
                b'&' => Err(self.err("unsupported atom: & (conjunction)", offset)),
                b'~' => Err(self.err("unsupported atom: ~ (last substitute string)", offset)),
                b'*' if star_literal_allowed => {
                    self.pos += 1;
                    Ok(Ast::Literal(b'*'))
                }
                b'*' => Err(self.err("E64: * follows nothing", offset)),
                b'+' | b'?' | b'=' => Err(self.err("E64: quantifier follows nothing", offset)),
                b'{' => Err(self.err("E64: { follows nothing", offset)),
                _ => {
                    self.pos += 1;
                    Ok(Ast::Literal(byte))
                }
            },
            Magic::Magic => match byte {
                b'.' => {
                    self.pos += 1;
                    Ok(Ast::Any)
                }
                b'[' => self.parse_class(false),
                b'^' if self.at_branch_start => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::LineStart))
                }
                b'$' if self.dollar_is_special() => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::LineEnd))
                }
                b'*' if star_literal_allowed => {
                    self.pos += 1;
                    Ok(Ast::Literal(b'*'))
                }
                b'*' => Err(self.err("E64: * follows nothing", offset)),
                b'~' => Err(self.err("unsupported atom: ~ (last substitute string)", offset)),
                _ => {
                    self.pos += 1;
                    Ok(Ast::Literal(byte))
                }
            },
            Magic::Nomagic => match byte {
                b'^' if self.at_branch_start => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::LineStart))
                }
                b'$' if self.dollar_is_special() => {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::LineEnd))
                }
                _ => {
                    self.pos += 1;
                    Ok(Ast::Literal(byte))
                }
            },
            Magic::VeryNomagic => {
                self.pos += 1;
                Ok(Ast::Literal(byte))
            }
        }
    }

    fn parse_escape(&mut self) -> Result<Ast, RegexError> {
        let offset = self.pos;
        self.pos += 1;
        let byte = match self.peek() {
            Some(byte) => byte,
            None => return Err(self.err("E14: Trailing backslash", offset)),
        };

        // Magic switches work in every mode and are zero width.
        match byte {
            b'v' => {
                self.magic = Magic::VeryMagic;
                self.pos += 1;
                return Ok(Ast::Empty);
            }
            b'm' => {
                self.magic = Magic::Magic;
                self.pos += 1;
                return Ok(Ast::Empty);
            }
            b'M' => {
                self.magic = Magic::Nomagic;
                self.pos += 1;
                return Ok(Ast::Empty);
            }
            b'V' => {
                self.magic = Magic::VeryNomagic;
                self.pos += 1;
                return Ok(Ast::Empty);
            }
            b'c' => {
                self.saw_ignorecase = true;
                self.pos += 1;
                return Ok(Ast::Empty);
            }
            b'C' => {
                self.saw_matchcase = true;
                self.pos += 1;
                return Ok(Ast::Empty);
            }
            _ => {}
        }

        if byte.is_ascii_digit() && byte != b'0' {
            let index = usize::from(byte - b'0');
            if index > self.groups {
                return Err(self.err("E65: Illegal back reference", offset));
            }
            self.pos += 1;
            return Ok(Ast::Backref(index));
        }

        match byte {
            b'e' => {
                self.pos += 1;
                Ok(Ast::Literal(0x1b))
            }
            b't' => {
                self.pos += 1;
                Ok(Ast::Literal(b'\t'))
            }
            b'r' => {
                self.pos += 1;
                Ok(Ast::Literal(b'\r'))
            }
            b'b' => {
                self.pos += 1;
                Ok(Ast::Literal(0x08))
            }
            b'n' => {
                self.pos += 1;
                Ok(Ast::Literal(b'\n'))
            }
            b'_' => self.parse_underscore(offset),
            b'z' => {
                self.pos += 1;
                // Vim also accepts a redundant backslash: "\z\(".
                if self.peek() == Some(b'\\') && self.peek_at(1) == Some(b'(') {
                    self.pos += 1;
                }
                match self.peek() {
                    Some(b'(') => self.parse_z_group(offset),
                    Some(b's') => {
                        self.pos += 1;
                        Ok(Ast::SetStart)
                    }
                    Some(b'e') => {
                        self.pos += 1;
                        Ok(Ast::SetEnd)
                    }
                    _ => Err(self.err("unsupported atom: \\z (syntax group)", offset)),
                }
            }
            // \Z matches composing characters; this byte-oriented engine has
            // no composing-character model, so approximate it with a no-op.
            b'Z' => {
                self.pos += 1;
                Ok(Ast::Empty)
            }
            b'%' => {
                if self.magic == Magic::VeryMagic {
                    self.pos += 1;
                    Ok(Ast::Literal(b'%'))
                } else {
                    self.parse_percent_escaped(offset)
                }
            }
            b'[' => {
                if self.magic == Magic::VeryMagic || self.magic == Magic::Magic {
                    self.pos += 1;
                    Ok(Ast::Literal(b'['))
                } else {
                    self.parse_class(false)
                }
            }
            b'.' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic || self.magic == Magic::Magic {
                    Ok(Ast::Literal(b'.'))
                } else {
                    Ok(Ast::Any)
                }
            }
            b'*' => {
                self.pos += 1;
                if self.magic == Magic::Magic || self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'*'))
                } else {
                    Err(self.err("E64: \\* follows nothing", offset))
                }
            }
            b'+' | b'?' | b'=' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(byte))
                } else {
                    Err(self.err("E64: quantifier follows nothing", offset))
                }
            }
            b'{' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'{'))
                } else {
                    Err(self.err("E64: \\{ follows nothing", offset))
                }
            }
            b'<' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'<'))
                } else {
                    Ok(Ast::Anchor(Anchor::WordStart))
                }
            }
            b'>' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'>'))
                } else {
                    Ok(Ast::Anchor(Anchor::WordEnd))
                }
            }
            b'^' => {
                self.pos += 1;
                if self.magic == Magic::VeryNomagic {
                    Ok(Ast::Anchor(Anchor::LineStart))
                } else {
                    Ok(Ast::Literal(b'^'))
                }
            }
            b'$' => {
                self.pos += 1;
                if self.magic == Magic::VeryNomagic {
                    Ok(Ast::Anchor(Anchor::LineEnd))
                } else {
                    Ok(Ast::Literal(b'$'))
                }
            }
            b'(' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'('))
                } else {
                    self.parse_group(true, offset)
                }
            }
            b')' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b')'))
                } else {
                    Err(self.err("E55: Unmatched \\)", offset))
                }
            }
            b'|' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'|'))
                } else {
                    Err(self.err("E55: Unexpected \\|", offset))
                }
            }
            b'@' => {
                if self.magic == Magic::VeryMagic {
                    self.pos += 1;
                    Ok(Ast::Literal(b'@'))
                } else {
                    match self.peek_lookaround()? {
                        Some(_) => Err(self.err("E59: look-around follows nothing", offset)),
                        None => Err(self.err("E59: Invalid character after @", offset)),
                    }
                }
            }
            b'&' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic {
                    Ok(Ast::Literal(b'&'))
                } else {
                    Err(self.err("unsupported atom: \\& (conjunction)", offset))
                }
            }
            b'~' => {
                self.pos += 1;
                if self.magic == Magic::VeryMagic || self.magic == Magic::Magic {
                    Ok(Ast::Literal(b'~'))
                } else {
                    Err(self.err("unsupported atom: \\~ (last substitute string)", offset))
                }
            }
            b'\\' => {
                self.pos += 1;
                Ok(Ast::Literal(b'\\'))
            }
            _ => {
                if let Some(class) = charclass::class_atom(byte, false) {
                    self.pos += 1;
                    return Ok(Ast::Class(class));
                }
                self.pos += 1;
                Ok(Ast::Literal(byte))
            }
        }
    }

    fn parse_underscore(&mut self, offset: usize) -> Result<Ast, RegexError> {
        self.pos += 1; // '_'
        let byte = match self.peek() {
            Some(byte) => byte,
            None => return Err(self.err("E63: Invalid use of \\_", offset)),
        };
        match byte {
            b'.' => {
                self.pos += 1;
                Ok(Ast::AnyNewline)
            }
            b'^' => {
                self.pos += 1;
                Ok(Ast::Anchor(Anchor::LineStart))
            }
            b'$' => {
                self.pos += 1;
                Ok(Ast::Anchor(Anchor::LineEnd))
            }
            b'[' => self.parse_class(true),
            _ => {
                if let Some(class) = charclass::class_atom(byte, true) {
                    self.pos += 1;
                    Ok(Ast::Class(class))
                } else {
                    Err(self.err("E63: Invalid use of \\_", offset))
                }
            }
        }
    }

    fn parse_percent_escaped(&mut self, offset: usize) -> Result<Ast, RegexError> {
        self.pos += 1; // '%'
        let byte = match self.peek() {
            Some(byte) => byte,
            None => return Err(self.err("E71: Invalid character after %", offset)),
        };
        match byte {
            b'(' => {
                self.pos += 1;
                self.parse_group(false, offset)
            }
            // Vim also accepts a redundant backslash: "\%\(pattern\)".
            b'\\' if self.peek_at(1) == Some(b'(') => {
                self.pos += 2;
                self.parse_group(false, offset)
            }
            b'^' => {
                self.pos += 1;
                Ok(Ast::Anchor(Anchor::TextStart))
            }
            b'$' => {
                self.pos += 1;
                Ok(Ast::Anchor(Anchor::TextEnd))
            }
            b'[' => {
                self.pos += 1;
                self.parse_optional_sequence(offset)
            }
            b'#' => {
                // \%#=1 and \%#=2 select a regexp engine and are zero width.
                if self.peek_at(1) == Some(b'=')
                    && matches!(self.peek_at(2), Some(b'1') | Some(b'2'))
                {
                    self.pos += 3;
                    Ok(Ast::Empty)
                } else {
                    self.pos += 1;
                    Ok(Ast::Anchor(Anchor::Never))
                }
            }
            // Contextual atoms need editor state this engine does not track;
            // they compile but never match.
            b'V' | b'C' => {
                self.pos += 1;
                Ok(Ast::Anchor(Anchor::Never))
            }
            b'\'' => {
                self.pos += 1;
                if self.peek().is_some() {
                    self.pos += 1;
                }
                Ok(Ast::Anchor(Anchor::Never))
            }
            b'<' | b'>' => {
                self.pos += 1;
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                }
                while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    self.pos += 1;
                }
                if matches!(self.peek(), Some(b'l') | Some(b'c') | Some(b'v')) {
                    self.pos += 1;
                }
                Ok(Ast::Anchor(Anchor::Never))
            }
            b'd' => self.parse_percent_char(offset, 10, usize::MAX),
            b'o' => self.parse_percent_char(offset, 8, usize::MAX),
            b'x' => self.parse_percent_char(offset, 16, 2),
            b'u' => self.parse_percent_char(offset, 16, 4),
            b'U' => self.parse_percent_char(offset, 16, 8),
            _ => {
                if byte.is_ascii_digit() {
                    // \%5l, \%5c, \%5v: line/column atoms.
                    while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                        self.pos += 1;
                    }
                    if matches!(self.peek(), Some(b'l') | Some(b'c') | Some(b'v')) {
                        self.pos += 1;
                    }
                    Ok(Ast::Anchor(Anchor::Never))
                } else {
                    Err(self.err("E71: Invalid character after %", offset))
                }
            }
        }
    }

    /// Parse \%[atoms]: match the longest possible prefix of the atoms.
    fn parse_optional_sequence(&mut self, offset: usize) -> Result<Ast, RegexError> {
        let mut items = Vec::new();
        loop {
            match self.peek() {
                None => return Err(self.err("E69: Missing ] after \\%[", offset)),
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => {}
            }
            items.push(self.parse_atom(false)?);
        }
        if items.is_empty() {
            return Err(self.err("E70: Empty \\%[]", offset));
        }
        let count = items.len();
        let mut alternatives = Vec::new();
        for length in (1..=count).rev() {
            let parts: Vec<Ast> = items[..length].to_vec();
            alternatives.push(if parts.len() == 1 {
                parts[0].clone()
            } else {
                Ast::Concat(parts)
            });
        }
        Ok(Ast::Repeat {
            node: Box::new(Ast::Alt(alternatives)),
            min: 0,
            max: Some(1),
            greedy: true,
        })
    }

    /// Parse a \%d / \%o / \%x / \%u / \%U character atom.
    fn parse_percent_char(
        &mut self,
        offset: usize,
        radix: u32,
        max_digits: usize,
    ) -> Result<Ast, RegexError> {
        self.pos += 1; // the base letter
        let mut value: u32 = 0;
        let mut digits = 0usize;
        while digits < max_digits {
            let Some(byte) = self.peek() else { break };
            let Some(digit) = (byte as char).to_digit(radix) else {
                break;
            };
            value = value.saturating_mul(radix).saturating_add(digit);
            self.pos += 1;
            digits += 1;
        }
        if digits == 0 {
            return Err(self.err("E678: Invalid character after %", offset));
        }
        if value == 0 {
            return Ok(Ast::Literal(b'\n'));
        }
        let character = char::from_u32(value)
            .ok_or_else(|| self.err("E678: Invalid character after %", offset))?;
        let mut buffer = [0u8; 4];
        let encoded = character.encode_utf8(&mut buffer).as_bytes();
        if encoded.len() == 1 {
            return Ok(Ast::Literal(encoded[0]));
        }
        Ok(Ast::Concat(
            encoded.iter().map(|byte| Ast::Literal(*byte)).collect(),
        ))
    }

    /// Parse \z( ... \): a capturing group usable from later syntax patterns.
    fn parse_z_group(&mut self, open_offset: usize) -> Result<Ast, RegexError> {
        self.pos += 1; // '('
        let index = self.groups + 1;
        self.groups += 1;
        self.z_groups.push(index);
        self.at_branch_start = true;
        let body = self.parse_alt()?;
        if !self.at_group_close() {
            return Err(self.err("E54: Unmatched \\(", open_offset));
        }
        self.eat_group_close();
        self.at_branch_start = false;
        Ok(Ast::Group {
            index,
            node: Box::new(body),
        })
    }

    fn parse_percent_very_magic(&mut self, offset: usize) -> Result<Ast, RegexError> {
        if self.starts_with(b"%(") {
            self.pos += 2;
            self.parse_group(false, offset)
        } else if self.starts_with(b"%^") {
            self.pos += 2;
            Ok(Ast::Anchor(Anchor::TextStart))
        } else if self.starts_with(b"%$") {
            self.pos += 2;
            Ok(Ast::Anchor(Anchor::TextEnd))
        } else if self.starts_with(b"%[") {
            self.pos += 2;
            self.parse_optional_sequence(offset)
        } else if matches!(
            self.peek_at(1),
            Some(b'0'..=b'9') | Some(b'<') | Some(b'>') | Some(b'#') | Some(b'V')
        ) {
            // In very magic "%" takes the role of "\%": accept the
            // line/column/cursor atoms.  They need editor context, so they
            // compile as never-matching anchors.
            self.pos += 1; // '%'
            if matches!(self.peek(), Some(b'<') | Some(b'>')) {
                self.pos += 1;
                if self.peek() == Some(b'.') {
                    self.pos += 1;
                }
            }
            if self.peek() == Some(b'#') {
                self.pos += 1;
                if self.peek() == Some(b'=') && self.peek_at(1).is_some() {
                    self.pos += 2;
                    return Ok(Ast::Empty);
                }
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.pos += 1;
            }
            if matches!(self.peek(), Some(b'l') | Some(b'c') | Some(b'v')) {
                self.pos += 1;
            }
            Ok(Ast::Anchor(Anchor::Never))
        } else {
            Err(self.err("E71: Invalid character after %", offset))
        }
    }

    fn parse_group(&mut self, capturing: bool, open_offset: usize) -> Result<Ast, RegexError> {
        let index = if capturing {
            if self.groups >= 9 {
                return Err(self.err("E51: Too many \\(", open_offset));
            }
            self.groups += 1;
            self.groups
        } else {
            0
        };
        self.at_branch_start = true;
        let body = self.parse_alt()?;
        if !self.at_group_close() {
            return Err(self.err("E54: Unmatched \\(", open_offset));
        }
        self.eat_group_close();
        self.at_branch_start = false;
        if capturing {
            Ok(Ast::Group {
                index,
                node: Box::new(body),
            })
        } else {
            Ok(body)
        }
    }

    fn peek_quantifier(&self) -> Option<QuantToken> {
        let offset = self.pos;
        match self.magic {
            Magic::VeryMagic => match self.peek() {
                Some(b'*') => Some(QuantToken {
                    kind: QuantKind::Star,
                    offset,
                    len: 1,
                }),
                Some(b'+') => Some(QuantToken {
                    kind: QuantKind::Plus,
                    offset,
                    len: 1,
                }),
                Some(b'?') | Some(b'=') => Some(QuantToken {
                    kind: QuantKind::Optional,
                    offset,
                    len: 1,
                }),
                Some(b'{') => Some(QuantToken {
                    kind: QuantKind::Interval,
                    offset,
                    len: 1,
                }),
                _ => None,
            },
            Magic::Magic => match self.peek() {
                Some(b'*') => Some(QuantToken {
                    kind: QuantKind::Star,
                    offset,
                    len: 1,
                }),
                _ => self.peek_escaped_quantifier(offset, false),
            },
            Magic::Nomagic | Magic::VeryNomagic => self.peek_escaped_quantifier(offset, true),
        }
    }

    fn peek_escaped_quantifier(&self, offset: usize, include_star: bool) -> Option<QuantToken> {
        match (self.peek(), self.peek_at(1)) {
            (Some(b'\\'), Some(b'*')) if include_star => Some(QuantToken {
                kind: QuantKind::Star,
                offset,
                len: 2,
            }),
            (Some(b'\\'), Some(b'+')) => Some(QuantToken {
                kind: QuantKind::Plus,
                offset,
                len: 2,
            }),
            (Some(b'\\'), Some(b'?')) | (Some(b'\\'), Some(b'=')) => Some(QuantToken {
                kind: QuantKind::Optional,
                offset,
                len: 2,
            }),
            (Some(b'\\'), Some(b'{')) => Some(QuantToken {
                kind: QuantKind::Interval,
                offset,
                len: 2,
            }),
            _ => None,
        }
    }

    fn consume_quantifier(
        &mut self,
        token: QuantToken,
    ) -> Result<(u32, Option<u32>, bool), RegexError> {
        self.pos += token.len;
        match token.kind {
            QuantKind::Star => Ok((0, None, true)),
            QuantKind::Plus => Ok((1, None, true)),
            QuantKind::Optional => Ok((0, Some(1), true)),
            QuantKind::Interval => self.parse_interval_body(token.offset),
        }
    }

    fn parse_interval_body(
        &mut self,
        offset: usize,
    ) -> Result<(u32, Option<u32>, bool), RegexError> {
        let lazy = if self.peek() == Some(b'-') {
            self.pos += 1;
            true
        } else {
            false
        };
        let first = self.parse_decimal()?;
        let mut min = first.unwrap_or(0);
        let mut max;
        let mut saw_any = first.is_some();
        if self.peek() == Some(b',') {
            self.pos += 1;
            let second = self.parse_decimal()?;
            saw_any = saw_any || second.is_some();
            if let Some(value) = second {
                if value < min {
                    return Err(self.err("E554: Min greater than max in \\{...}", offset));
                }
                max = Some(value);
            } else {
                max = None;
            }
        } else if first.is_some() {
            max = Some(min);
        } else {
            max = None;
        }
        if !saw_any {
            // \{} and \{-} mean 0 or more; a later stray token is rejected by
            // the closing-brace check below.
            min = 0;
            max = None;
        }
        if self.peek() == Some(b'\\') && self.peek_at(1) == Some(b'}') {
            self.pos += 2;
        } else if self.peek() == Some(b'}') {
            self.pos += 1;
        } else {
            return Err(self.err("E554: Syntax error in \\{...}", offset));
        }
        Ok((min, max, !lazy))
    }

    fn parse_decimal(&mut self) -> Result<Option<u32>, RegexError> {
        let offset = self.pos;
        let mut value: u32 = 0;
        let mut any = false;
        while let Some(byte) = self.peek() {
            if !byte.is_ascii_digit() {
                break;
            }
            any = true;
            let digit = u32::from(byte - b'0');
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .ok_or_else(|| self.err("E554: Number too large in \\{...}", offset))?;
            self.pos += 1;
        }
        Ok(if any { Some(value) } else { None })
    }

    fn peek_lookaround(&self) -> Result<Option<LookToken>, RegexError> {
        let prefix = match self.magic {
            Magic::VeryMagic => {
                if self.peek() == Some(b'@') {
                    1
                } else {
                    return Ok(None);
                }
            }
            _ => {
                if self.starts_with(b"\\@") {
                    2
                } else {
                    return Ok(None);
                }
            }
        };
        let offset = self.pos;
        let mut cursor = self.pos + prefix;
        let byte = match self.pattern.get(cursor).copied() {
            Some(byte) => byte,
            None => return Err(self.err("E59: Invalid character after @", offset)),
        };
        let kind = match byte {
            b'=' => {
                cursor += 1;
                LookTokenKind::Ahead
            }
            b'!' => {
                cursor += 1;
                LookTokenKind::NotAhead
            }
            b'>' => {
                cursor += 1;
                LookTokenKind::Atomic
            }
            b'<' => {
                cursor += 1;
                match self.pattern.get(cursor).copied() {
                    Some(b'=') => {
                        cursor += 1;
                        LookTokenKind::Behind(None)
                    }
                    Some(b'!') => {
                        cursor += 1;
                        LookTokenKind::NotBehind(None)
                    }
                    _ => return Err(self.err("E59: Invalid character after @", offset)),
                }
            }
            b'0'..=b'9' => {
                let digits_start = cursor;
                while self.pattern.get(cursor).is_some_and(u8::is_ascii_digit) {
                    cursor += 1;
                }
                let mut limit: usize = 0;
                for &digit in &self.pattern[digits_start..cursor] {
                    limit = limit
                        .checked_mul(10)
                        .and_then(|v| v.checked_add(usize::from(digit - b'0')))
                        .ok_or_else(|| self.err("E59: Look-behind limit too large", offset))?;
                }
                let limit = if limit == 0 { None } else { Some(limit) };
                // The numeric limit only matters for look-behind; "\@1=" is a
                // plain look-ahead (runtime/syntax/typst.vim relies on this).
                match self.pattern.get(cursor).copied() {
                    Some(b'=') => {
                        cursor += 1;
                        LookTokenKind::Ahead
                    }
                    Some(b'!') => {
                        cursor += 1;
                        LookTokenKind::NotAhead
                    }
                    Some(b'>') => {
                        cursor += 1;
                        LookTokenKind::Atomic
                    }
                    Some(b'<') => {
                        cursor += 1;
                        match self.pattern.get(cursor).copied() {
                            Some(b'=') => {
                                cursor += 1;
                                LookTokenKind::Behind(limit)
                            }
                            Some(b'!') => {
                                cursor += 1;
                                LookTokenKind::NotBehind(limit)
                            }
                            _ => return Err(self.err("E59: Invalid character after @", offset)),
                        }
                    }
                    _ => return Err(self.err("E59: Invalid character after @", offset)),
                }
            }
            _ => return Err(self.err("E59: Invalid character after @", offset)),
        };
        Ok(Some(LookToken {
            kind,
            offset,
            end: cursor,
        }))
    }

    fn multi_follows(&self) -> Result<bool, RegexError> {
        if self.peek_quantifier().is_some() {
            return Ok(true);
        }
        Ok(self.peek_lookaround()?.is_some())
    }

    fn at_alt(&self) -> bool {
        match self.magic {
            Magic::VeryMagic => self.peek() == Some(b'|'),
            _ => self.starts_with(b"\\|"),
        }
    }

    fn eat_alt(&mut self) -> bool {
        if !self.at_alt() {
            return false;
        }
        self.pos += if self.magic == Magic::VeryMagic { 1 } else { 2 };
        true
    }

    fn at_group_close(&self) -> bool {
        match self.magic {
            Magic::VeryMagic => self.peek() == Some(b')'),
            _ => self.starts_with(b"\\)"),
        }
    }

    fn eat_group_close(&mut self) -> bool {
        if !self.at_group_close() {
            return false;
        }
        self.pos += if self.magic == Magic::VeryMagic { 1 } else { 2 };
        true
    }

    fn at_concat_end(&self) -> bool {
        self.pos >= self.pattern.len()
            || self.at_alt()
            || self.at_group_close()
            || self.at_conjunction()
    }

    fn at_conjunction(&self) -> bool {
        match self.magic {
            Magic::VeryMagic => self.peek() == Some(b'&'),
            // "\&" is a magic atom in every other mode, including very nomagic.
            Magic::Magic | Magic::Nomagic | Magic::VeryNomagic => self.starts_with(b"\\&"),
        }
    }

    fn eat_conjunction(&mut self) -> bool {
        if !self.at_conjunction() {
            return false;
        }
        self.pos += if self.magic == Magic::VeryMagic { 1 } else { 2 };
        true
    }

    fn dollar_is_special(&self) -> bool {
        match self.magic {
            Magic::VeryMagic => true,
            Magic::VeryNomagic => false,
            Magic::Magic | Magic::Nomagic => {
                let rest = &self.pattern[self.pos + 1..];
                rest.is_empty()
                    || rest.starts_with(b"\\|")
                    || rest.starts_with(b"\\)")
                    || rest.starts_with(b"\\n")
            }
        }
    }

    fn parse_class(&mut self, extra_newline: bool) -> Result<Ast, RegexError> {
        let start = self.pos;
        debug_assert_eq!(self.peek(), Some(b'['));
        self.pos += 1;
        let negated = if self.peek() == Some(b'^') {
            self.pos += 1;
            true
        } else {
            false
        };
        let mut set = ByteSet::empty();
        let mut first = true;
        let mut saw_newline = false;
        loop {
            let byte = match self.peek() {
                Some(byte) => byte,
                None => return Ok(self.class_fallback(start)),
            };
            if byte == b']' && !first {
                self.pos += 1;
                let include_newline = extra_newline || saw_newline;
                return Ok(Ast::Class(CharClass::collection(
                    set,
                    negated,
                    include_newline,
                )));
            }
            if first && byte == b']' {
                set.insert(b']');
                self.pos += 1;
                first = false;
                continue;
            }
            let item = match self.parse_class_item()? {
                Some(item) => item,
                None => return Ok(self.class_fallback(start)),
            };
            match item {
                ClassItem::Byte(low) => {
                    if self.peek() == Some(b'-')
                        && self.peek_at(1).is_some()
                        && self.peek_at(1) != Some(b']')
                    {
                        let dash_offset = self.pos;
                        self.pos += 1;
                        let end_item = match self.parse_class_item()? {
                            Some(item) => item,
                            None => return Ok(self.class_fallback(start)),
                        };
                        match end_item {
                            ClassItem::Byte(high) => {
                                if low > high {
                                    // Vim's legacy engine accepts such patterns;
                                    // treat the three characters as literals
                                    // instead of rejecting the whole syntax file.
                                    set.insert(low);
                                    set.insert(b'-');
                                    set.insert(high);
                                    continue;
                                }
                                if low <= b'\n' && b'\n' <= high {
                                    saw_newline = true;
                                }
                                set.insert_range(low, high);
                            }
                            ClassItem::Set(_) => {
                                return Err(self.err(
                                    "E944: Invalid range end in character class",
                                    dash_offset,
                                ));
                            }
                        }
                    } else {
                        if low == b'\n' {
                            saw_newline = true;
                        }
                        set.insert(low);
                    }
                }
                ClassItem::Set(other) => set.union(&other),
            }
            first = false;
        }
    }

    fn class_fallback(&mut self, start: usize) -> Ast {
        self.pos = start + 1;
        Ast::Literal(b'[')
    }

    fn parse_class_item(&mut self) -> Result<Option<ClassItem>, RegexError> {
        let start = self.pos;
        let byte = match self.peek() {
            Some(byte) => byte,
            None => return Ok(None),
        };
        if byte == b'\\' {
            return self.parse_class_escape();
        }
        if byte == b'[' {
            match self.peek_at(1) {
                Some(b':') => return self.parse_posix_class(start),
                Some(b'=') => return self.parse_equivalence(start, b'='),
                Some(b'.') => return self.parse_equivalence(start, b'.'),
                _ => {}
            }
        }
        self.pos += 1;
        Ok(Some(ClassItem::Byte(byte)))
    }

    fn parse_posix_class(&mut self, start: usize) -> Result<Option<ClassItem>, RegexError> {
        let mut cursor = start + 2;
        let mut end = None;
        while cursor + 1 < self.pattern.len() {
            if self.pattern[cursor] == b':' && self.pattern[cursor + 1] == b']' {
                end = Some(cursor);
                break;
            }
            cursor += 1;
        }
        let end = match end {
            Some(end) => end,
            None => return Ok(None),
        };
        let name = &self.pattern[start + 2..end];
        let set = charclass::posix_class(name)
            .ok_or_else(|| self.err("E1030: Unknown character class name", start))?;
        self.pos = end + 2;
        Ok(Some(ClassItem::Set(set)))
    }

    fn parse_equivalence(
        &mut self,
        start: usize,
        close: u8,
    ) -> Result<Option<ClassItem>, RegexError> {
        let mut cursor = start + 2;
        let mut end = None;
        while cursor + 1 < self.pattern.len() {
            if self.pattern[cursor] == close && self.pattern[cursor + 1] == b']' {
                end = Some(cursor);
                break;
            }
            cursor += 1;
        }
        let end = match end {
            Some(end) => end,
            None => return Ok(None),
        };
        let contents = &self.pattern[start + 2..end];
        if contents.len() != 1 {
            return Err(self.err("E945: Equivalence/collation class needs one byte", start));
        }
        self.pos = end + 2;
        Ok(Some(ClassItem::Byte(contents[0])))
    }

    fn parse_class_escape(&mut self) -> Result<Option<ClassItem>, RegexError> {
        self.pos += 1;
        let byte = match self.peek() {
            Some(byte) => byte,
            None => return Ok(None),
        };
        self.pos += 1;
        match byte {
            b'e' => Ok(Some(ClassItem::Byte(0x1b))),
            b't' => Ok(Some(ClassItem::Byte(b'\t'))),
            b'r' => Ok(Some(ClassItem::Byte(b'\r'))),
            b'b' => Ok(Some(ClassItem::Byte(0x08))),
            b'n' => Ok(Some(ClassItem::Byte(b'\n'))),
            b'\\' | b']' | b'^' | b'-' | b'[' => Ok(Some(ClassItem::Byte(byte))),
            b'd' => self.parse_numeric_class_escape(10, usize::MAX, b'd'),
            b'o' => self.parse_numeric_class_escape(8, usize::MAX, b'o'),
            b'x' => self.parse_numeric_class_escape(16, 2, b'x'),
            b'u' => self.parse_numeric_class_escape(16, 4, b'u'),
            b'U' => self.parse_numeric_class_escape(16, 8, b'U'),
            _ => Ok(Some(ClassItem::Byte(byte))),
        }
    }

    fn parse_numeric_class_escape(
        &mut self,
        base: u32,
        max_digits: usize,
        fallback: u8,
    ) -> Result<Option<ClassItem>, RegexError> {
        let start = self.pos;
        let mut value: u32 = 0;
        let mut digits = 0;
        while digits < max_digits {
            let Some(byte) = self.peek() else {
                break;
            };
            let digit = match byte {
                b'0'..=b'9' => u32::from(byte - b'0'),
                b'a'..=b'f' => u32::from(byte - b'a') + 10,
                b'A'..=b'F' => u32::from(byte - b'A') + 10,
                _ => break,
            };
            if digit >= base {
                break;
            }
            value = value.saturating_mul(base).saturating_add(digit);
            self.pos += 1;
            digits += 1;
        }
        if digits == 0 {
            // Not a numeric escape after all; the character is literal.
            self.pos = start;
            return Ok(Some(ClassItem::Byte(fallback)));
        }
        if value > 255 {
            // Byte-oriented engine: approximate a multi-byte character by
            // accepting any of its UTF-8 bytes inside the collection.
            let Some(character) = char::from_u32(value) else {
                return Err(self.err(
                    "character outside Unicode range in character class",
                    start.saturating_sub(2),
                ));
            };
            let mut buffer = [0u8; 4];
            let encoded = character.encode_utf8(&mut buffer);
            let mut set = ByteSet::empty();
            for byte in encoded.as_bytes() {
                set.insert(*byte);
            }
            return Ok(Some(ClassItem::Set(set)));
        }
        Ok(Some(ClassItem::Byte(value as u8)))
    }

    fn peek(&self) -> Option<u8> {
        self.pattern.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.pattern.get(self.pos + offset).copied()
    }

    fn starts_with(&self, needle: &[u8]) -> bool {
        self.pattern[self.pos..].starts_with(needle)
    }

    fn err(&self, message: impl Into<String>, offset: usize) -> RegexError {
        RegexError::new(message, offset)
    }
}
