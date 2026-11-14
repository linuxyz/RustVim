//! Vim9 script lexer.
//!
//! Byte oriented, like the rest of the project: source text is never required
//! to be UTF-8.  The lexer turns a whole script into a token stream (see
//! token.rs for the contract).  It is deliberately line aware:
//!
//! * a Newline token is emitted for every physical line end; the parser
//!   decides where a newline continues an expression,
//! * a line whose first non-blank character is a backslash is joined to the
//!   previous line (this is what Vim's script reader does before parsing; the
//!   backslash is dropped), and the same joining applies inside strings,
//! * comments start with '#' at the start of a line or after space/tab and are
//!   consumed up to (but not including) the line end,
//! * "=&lt;&lt;" starts a heredoc; the lexer emits Punct::Assign for the '=' and then
//!   a Heredoc token followed by a synthetic Newline token for the line after
//!   the end marker.
//!
//! Behaviours marked "oracle" in the tests were verified with the local
//! /usr/bin/vim 9.1 using: vim -Nu NONE -n -es -i NONE -S file -c 'qa!'.

use std::collections::VecDeque;

use rvim_types::VimString;

use crate::error::{Result, VimError};
use crate::lexer::Scope;

use super::error::{codes, error};
use super::token::{Heredoc, HeredocFlags, InterpPart, Punct, Token, TokenKind};

/// Return the byte range of the physical line containing "pos":
/// (line start, end excluding the newline, start of the next line).
///
/// CRLF counts as a single newline and a lone CR is treated as one too, so
/// "end" excludes a trailing CR and "next" skips the whole line ending.
pub fn line_bounds(input: &[u8], pos: usize) -> (usize, usize, usize) {
    let pos = pos.min(input.len());
    let mut start = pos;
    while start > 0 {
        let previous = input[start - 1];
        if previous == b'\n' {
            break;
        }
        // A CR that is not part of a CRLF line ending terminates a line.
        if previous == b'\r' && input.get(start).copied() != Some(b'\n') {
            break;
        }
        start -= 1;
    }
    let mut end = start;
    while end < input.len() && newline_len(input, end) == 0 {
        end += 1;
    }
    let next = end + newline_len(input, end);
    (start, end, next)
}

/// Length of the line ending at "pos": 0, 1 or 2 bytes (CRLF).
fn newline_len(input: &[u8], pos: usize) -> usize {
    match input.get(pos) {
        Some(b'\n') => 1,
        Some(b'\r') => {
            if input.get(pos + 1) == Some(&b'\n') {
                2
            } else {
                1
            }
        }
        _ => 0,
    }
}

/// Tokenize a whole Vim9 script.
pub fn tokenize(input: &[u8]) -> Result<Vec<Token>> {
    let mut lexer = Lexer::new(input);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.next_token()?;
        let eof = matches!(token.kind, TokenKind::Eof);
        tokens.push(token);
        if eof {
            break;
        }
    }
    Ok(tokens)
}

/// Lexer cursor over Vim9 source.  The parser uses it incrementally so that
/// Ex command lines can be captured verbatim without being tokenized.
pub struct Lexer<'a> {
    pub(crate) input: &'a [u8],
    pub(crate) pos: usize,
    pub(crate) line: u32,
    pub(crate) col: u32,
    /// Tokens produced eagerly for "=&lt;&lt;" (Assign, Heredoc, Newline).
    queue: VecDeque<Token>,
    /// Whether a token was already produced on the current logical line.
    line_has_token: bool,
    /// The last identifier token emitted on the current physical line.
    last_ident: Option<String>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a [u8]) -> Self {
        Lexer {
            input,
            pos: 0,
            line: 1,
            col: 1,
            queue: VecDeque::new(),
            line_has_token: false,
            last_ident: None,
        }
    }

    pub fn source(&self) -> &'a [u8] {
        self.input
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn line(&self) -> u32 {
        self.line
    }

    /// Byte column of the cursor (1 based).
    pub fn col(&self) -> u32 {
        self.col
    }

    /// Raw bytes from the cursor to the end of the physical line.
    ///
    /// The parser uses this to inspect the text after a name token when it
    /// decides whether a line is a Vim9 statement or an Ex command.
    pub fn rest_of_line(&self) -> &'a [u8] {
        let (_, end, _) = line_bounds(self.input, self.pos);
        &self.input[self.pos.min(end)..end]
    }

    /// Reposition the cursor.  The caller supplies the physical line number of
    /// "pos" (it is always the line after a captured Ex command line).
    pub fn seek(&mut self, pos: usize, line: u32) {
        self.queue.clear();
        let pos = pos.min(self.input.len());
        self.pos = pos;
        self.line = line.max(1);
        self.col = 1;
        let mut start = pos;
        while start > 0 && newline_len(self.input, start - 1) == 0 {
            start -= 1;
        }
        // "start - 1" is inside a line ending (or before a lone CR).
        let line_start = if start > 0 {
            start - 1 + newline_len(self.input, start - 1)
        } else {
            0
        };
        self.col = (pos - line_start + 1) as u32;
        self.line_has_token = false;
        self.last_ident = None;
    }

    pub fn next_token(&mut self) -> Result<Token> {
        if let Some(token) = self.queue.pop_front() {
            self.line_has_token = !matches!(token.kind, TokenKind::Newline | TokenKind::Eof);
            if matches!(token.kind, TokenKind::Newline | TokenKind::Eof) {
                self.last_ident = None;
            }
            return Ok(token);
        }
        loop {
            // White space and redundant backslashes are skipped.  A backslash
            // outside a string only appears as an Ex command continuation
            // marker; Vim's script reader drops it and joins the lines, so the
            // lexer ignores it too.
            let mut had_space = false;
            while let Some(byte) = self.peek(0) {
                if matches!(byte, b' ' | b'\t' | b'\\') {
                    self.advance();
                    had_space = true;
                } else {
                    break;
                }
            }
            if self.pos >= self.input.len() {
                self.line_has_token = false;
                return Ok(self.make_token(TokenKind::Eof, self.pos, self.line, self.col, false));
            }
            let start = self.pos;
            let start_line = self.line;
            let start_col = self.col;
            if newline_len(self.input, self.pos) > 0 {
                if let Some(target) = self.continuation_target() {
                    self.join_continuation(target);
                    continue;
                }
                let end = self.pos + newline_len(self.input, self.pos);
                self.consume_newline();
                self.line_has_token = false;
                self.last_ident = None;
                return Ok(Token {
                    kind: TokenKind::Newline,
                    start,
                    end,
                    line: start_line,
                    col: start_col,
                    space_before: false,
                });
            }
            let space_before = had_space && self.line_has_token;
            // Vim tolerates a legacy quote comment after a compiled block end
            // command (runtime files contain "endif \"}}}").  Everywhere else
            // a double quote starts a string.
            if self.input[self.pos] == b'"'
                && space_before
                && matches!(
                    self.last_ident.as_deref(),
                    Some("endif" | "endwhile" | "endfor" | "endtry")
                )
            {
                self.skip_to_line_end();
                continue;
            }
            // A '#' starts a comment at the start of a line or after white
            // space.  Elsewhere it is either part of an autoload name (handled
            // while scanning names) or an error.
            if self.input[self.pos] == b'#' && (space_before || !self.line_has_token) {
                if self.peek(1) == Some(b'{') && self.peek(2) != Some(b'{') {
                    return Err(error(1170, "Cannot use #{ to start a comment", start_line));
                }
                self.skip_to_line_end();
                continue;
            }
            let token = self.lex_token(start, start_line, start_col, space_before)?;
            self.line_has_token = !matches!(token.kind, TokenKind::Eof);
            self.last_ident = match &token.kind {
                TokenKind::Ident(name) => Some(name.to_string_lossy().into_owned()),
                _ => None,
            };
            return Ok(token);
        }
    }

    // -----------------------------------------------------------------
    // Cursor helpers
    // -----------------------------------------------------------------

    fn peek(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    fn advance(&mut self) {
        if let Some(byte) = self.input.get(self.pos).copied() {
            self.pos += 1;
            if byte == b'\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
        }
    }

    fn advance_by(&mut self, count: usize) {
        for _ in 0..count {
            self.advance();
        }
    }

    fn advance_to(&mut self, target: usize) {
        while self.pos < target {
            self.advance();
        }
    }

    fn consume_newline(&mut self) {
        self.pos += newline_len(self.input, self.pos);
        self.line += 1;
        self.col = 1;
    }

    fn make_token(
        &self,
        kind: TokenKind,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Token {
        Token {
            kind,
            start,
            end: self.pos,
            line,
            col,
            space_before,
        }
    }

    /// If the current byte is a line ending followed by a continued line
    /// (white space, then a backslash), return the index just after the
    /// backslash.
    fn continuation_target(&self) -> Option<usize> {
        let mut index = self.pos + newline_len(self.input, self.pos);
        while matches!(self.input.get(index), Some(b' ' | b'\t')) {
            index += 1;
        }
        if self.input.get(index) == Some(&b'\\') {
            Some(index + 1)
        } else {
            None
        }
    }

    /// Join a continued line: drop the line ending, leading blanks and the
    /// backslash, like Vim's getsourceline() does before parsing.
    fn join_continuation(&mut self, target: usize) {
        let line_start = self.pos + newline_len(self.input, self.pos);
        self.pos = target;
        self.line += 1;
        self.col = (target - line_start + 1) as u32;
    }

    fn skip_to_line_end(&mut self) {
        while self.pos < self.input.len() && newline_len(self.input, self.pos) == 0 {
            self.advance();
        }
    }

    fn line_end(&self, from: usize) -> usize {
        let mut end = from;
        while end < self.input.len() && newline_len(self.input, end) == 0 {
            end += 1;
        }
        end
    }

    /// Text from "from" to the end of its line, for error messages.
    fn line_snippet(&self, from: usize) -> String {
        let end = self.line_end(from.min(self.input.len()));
        String::from_utf8_lossy(&self.input[from.min(end)..end])
            .trim()
            .to_string()
    }

    // -----------------------------------------------------------------
    // One token
    // -----------------------------------------------------------------

    fn lex_token(
        &mut self,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        let byte = self.input[self.pos];
        if byte.is_ascii_digit() {
            return self.number(start, line, col, space_before);
        }
        if byte == b'.' && self.peek(1).is_some_and(|next| next.is_ascii_digit()) {
            return self.number(start, line, col, space_before);
        }
        match byte {
            b'\'' | b'"' => self.quoted_string(byte, start, line, col, space_before),
            b'$' => {
                if matches!(self.peek(1), Some(b'"') | Some(b'\'')) {
                    self.interpolated_string(start, line, col, space_before)
                } else {
                    self.env_var(start, line, col, space_before)
                }
            }
            b'@' => self.register(start, line, col, space_before),
            b'&' => {
                if self.peek(1) == Some(b'&') {
                    self.punct(Punct::AndAnd, 2, start, line, col, space_before)
                } else {
                    self.option(start, line, col, space_before)
                }
            }
            b'|' => {
                if self.peek(1) == Some(b'|') {
                    self.punct(Punct::OrOr, 2, start, line, col, space_before)
                } else {
                    self.punct(Punct::Bar, 1, start, line, col, space_before)
                }
            }
            b'=' if self.peek(1) == Some(b'<') && self.peek(2) == Some(b'<') => {
                self.heredoc(start, line, col, space_before)
            }
            _ if is_ident_start(byte) => self.identifier_or_scoped(start, line, col, space_before),
            _ => self.punct_token(start, line, col, space_before),
        }
    }

    fn punct(
        &mut self,
        kind: Punct,
        len: usize,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        self.advance_by(len);
        Ok(self.make_token(TokenKind::Punct(kind), start, line, col, space_before))
    }

    /// A comparison operator, consuming the optional '#' or '?' case suffix
    /// (Vim9 accepts it even though 'ignorecase' is not used).
    fn comparison(
        &mut self,
        kind: Punct,
        len: usize,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        self.advance_by(len);
        if matches!(self.peek(0), Some(b'#') | Some(b'?')) {
            self.advance();
        }
        Ok(self.make_token(TokenKind::Punct(kind), start, line, col, space_before))
    }

    fn simple_punct(
        &mut self,
        kind: Punct,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        self.punct(kind, 1, start, line, col, space_before)
    }

    fn comparison_start(
        &mut self,
        kind: Punct,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        self.comparison(kind, 1, start, line, col, space_before)
    }

    fn punct_token(
        &mut self,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        let byte = self.input[self.pos];
        let next = self.peek(1);
        match byte {
            b'+' => {
                if next == Some(b'=') {
                    self.punct(Punct::PlusAssign, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Plus, start, line, col, space_before)
                }
            }
            b'-' => {
                if next == Some(b'>') {
                    self.punct(Punct::Arrow, 2, start, line, col, space_before)
                } else if next == Some(b'=') {
                    self.punct(Punct::MinusAssign, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Minus, start, line, col, space_before)
                }
            }
            b'*' => {
                if next == Some(b'=') {
                    self.punct(Punct::StarAssign, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Star, start, line, col, space_before)
                }
            }
            b'/' => {
                if next == Some(b'=') {
                    self.punct(Punct::SlashAssign, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Slash, start, line, col, space_before)
                }
            }
            b'%' => {
                if next == Some(b'=') {
                    self.punct(Punct::PercentAssign, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Percent, start, line, col, space_before)
                }
            }
            b'.' => {
                if next == Some(b'.') && self.peek(2) == Some(b'.') {
                    self.punct(Punct::Ellipsis, 3, start, line, col, space_before)
                } else if next == Some(b'.') && self.peek(2) == Some(b'=') {
                    self.punct(Punct::DotDotAssign, 3, start, line, col, space_before)
                } else if next == Some(b'.') {
                    self.punct(Punct::DotDot, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Dot, start, line, col, space_before)
                }
            }
            b'=' => match next {
                Some(b'=') => self.comparison(Punct::EqEq, 2, start, line, col, space_before),
                Some(b'~') => self.comparison(Punct::Match, 2, start, line, col, space_before),
                Some(b'>') => self.punct(Punct::FatArrow, 2, start, line, col, space_before),
                _ => self.simple_punct(Punct::Assign, start, line, col, space_before),
            },
            b'!' => match next {
                Some(b'=') => self.comparison(Punct::NotEq, 2, start, line, col, space_before),
                Some(b'~') => self.comparison(Punct::NotMatch, 2, start, line, col, space_before),
                _ => self.simple_punct(Punct::Bang, start, line, col, space_before),
            },
            b'>' => {
                if next == Some(b'=') {
                    self.comparison(Punct::Ge, 2, start, line, col, space_before)
                } else {
                    self.comparison_start(Punct::Gt, start, line, col, space_before)
                }
            }
            b'<' => {
                if next == Some(b'=') {
                    self.comparison(Punct::Le, 2, start, line, col, space_before)
                } else {
                    self.comparison_start(Punct::Lt, start, line, col, space_before)
                }
            }
            b'?' => {
                if next == Some(b'?') {
                    self.punct(Punct::QuestionQuestion, 2, start, line, col, space_before)
                } else {
                    self.simple_punct(Punct::Question, start, line, col, space_before)
                }
            }
            b':' => self.simple_punct(Punct::Colon, start, line, col, space_before),
            b';' => self.simple_punct(Punct::Semicolon, start, line, col, space_before),
            b',' => self.simple_punct(Punct::Comma, start, line, col, space_before),
            b'(' => self.simple_punct(Punct::LParen, start, line, col, space_before),
            b')' => self.simple_punct(Punct::RParen, start, line, col, space_before),
            b'[' => self.simple_punct(Punct::LBracket, start, line, col, space_before),
            b']' => self.simple_punct(Punct::RBracket, start, line, col, space_before),
            b'{' => self.simple_punct(Punct::LBrace, start, line, col, space_before),
            b'}' => self.simple_punct(Punct::RBrace, start, line, col, space_before),
            _ => {
                let end = if self.pos < self.input.len() {
                    (self.pos + 1).min(self.line_end(self.pos))
                } else {
                    self.pos
                };
                let snippet = String::from_utf8_lossy(&self.input[self.pos..end])
                    .chars()
                    .next()
                    .map(|ch| format!("{ch:?}"))
                    .unwrap_or_else(|| "NUL".to_string());
                Err(error(488, format!("Trailing characters: {snippet}"), line))
            }
        }
    }

    // -----------------------------------------------------------------
    // Names, scopes, options, environment variables, registers
    // -----------------------------------------------------------------

    fn identifier_or_scoped(
        &mut self,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        let first = self.input[self.pos];
        if let Some(scope) = Scope::from_prefix(first) {
            if self.peek(1) == Some(b':') {
                match self.peek(2) {
                    Some(next) if is_ident_continue(next) => {
                        self.advance_by(2);
                        let name_start = self.pos;
                        while self.peek(0).is_some_and(is_ident_continue) {
                            self.advance();
                        }
                        let name = VimString::from(&self.input[name_start..self.pos]);
                        return Ok(self.make_token(
                            TokenKind::ScopedVar(scope, name),
                            start,
                            line,
                            col,
                            space_before,
                        ));
                    }
                    // "g:" followed by white space is a dictionary key like
                    // {g: 1}, not a scoped variable.  ("g:" at the end of a
                    // line or before punctuation is the whole-scope
                    // dictionary, e.g. get(g:, ...).)
                    Some(b' ') | Some(b'\t') => {}
                    _ => {
                        self.advance_by(2);
                        return Ok(self.make_token(
                            TokenKind::ScopedVar(scope, VimString::new()),
                            start,
                            line,
                            col,
                            space_before,
                        ));
                    }
                }
            }
        }
        let name_start = self.pos;
        while self.peek(0).is_some_and(is_ident_continue) {
            self.advance();
        }
        let name = VimString::from(&self.input[name_start..self.pos]);
        Ok(self.make_token(TokenKind::Ident(name), start, line, col, space_before))
    }

    fn option(&mut self, start: usize, line: u32, col: u32, space_before: bool) -> Result<Token> {
        self.advance(); // '&'
        let name_start = self.pos;
        if matches!(self.peek(0), Some(b'g') | Some(b'l')) && self.peek(1) == Some(b':') {
            self.advance_by(2);
        }
        let name_part = self.pos;
        if !self.peek(0).is_some_and(|byte| byte.is_ascii_alphabetic()) {
            return Err(error(112, "Option name missing", line));
        }
        // "&t_xx" is a termcap option: Vim takes exactly four characters.
        if self.input[name_part] == b't'
            && self.input.get(name_part + 1) == Some(&b'_')
            && self.input.get(name_part + 2).is_some()
            && self.input.get(name_part + 3).is_some()
        {
            self.advance_by(4);
        } else {
            while self.peek(0).is_some_and(|byte| byte.is_ascii_alphabetic()) {
                self.advance();
            }
        }
        let name = String::from_utf8_lossy(&self.input[name_start..self.pos]).into_owned();
        Ok(self.make_token(TokenKind::Option(name), start, line, col, space_before))
    }

    fn env_var(&mut self, start: usize, line: u32, col: u32, space_before: bool) -> Result<Token> {
        self.advance(); // '$'
        let name_start = self.pos;
        while self
            .peek(0)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            self.advance();
        }
        if self.pos == name_start {
            return Err(error(
                15,
                format!("Invalid expression: {}", self.line_snippet(start)),
                line,
            ));
        }
        let name = String::from_utf8_lossy(&self.input[name_start..self.pos]).into_owned();
        Ok(self.make_token(TokenKind::Env(name), start, line, col, space_before))
    }

    fn register(&mut self, start: usize, line: u32, col: u32, space_before: bool) -> Result<Token> {
        self.advance(); // '@'
        let Some(name) = self.peek(0) else {
            return Err(error(354, "Invalid register name", line));
        };
        if newline_len(self.input, self.pos) > 0 {
            return Err(error(354, "Invalid register name", line));
        }
        self.advance();
        Ok(self.make_token(TokenKind::Register(name), start, line, col, space_before))
    }

    // -----------------------------------------------------------------
    // Numbers, floats and blobs
    // -----------------------------------------------------------------

    /// End offset of a Vim9 float literal starting at the cursor, if any.
    ///
    /// Vim accepts "\[0-9\]*\.\[0-9\]+(\[eE\]\[+-\]?\[0-9\]+)?" (the leading digit may
    /// be omitted in Vim9) and rejects the float when it is followed by a
    /// letter or another '.': "1.5e" and "1.2.3" are not floats.
    fn float_end(&self) -> Option<usize> {
        let input = self.input;
        let len = input.len();
        let pos = self.pos;
        let mut p;
        if input[pos] == b'.' {
            p = pos;
        } else {
            p = pos + 1;
            loop {
                if input.get(p) == Some(&b'\'') {
                    p += 1;
                }
                if p >= len || !input[p].is_ascii_digit() {
                    break;
                }
                while p < len && input[p].is_ascii_digit() {
                    p += 1;
                }
            }
        }
        if p >= len || input[p] != b'.' || p + 1 >= len || !input[p + 1].is_ascii_digit() {
            return None;
        }
        p += 2;
        while p < len && input[p].is_ascii_digit() {
            p += 1;
        }
        if p < len && matches!(input[p], b'e' | b'E') {
            p += 1;
            if p < len && matches!(input[p], b'+' | b'-') {
                p += 1;
            }
            if p >= len || !input[p].is_ascii_digit() {
                return None;
            }
            while p < len && input[p].is_ascii_digit() {
                p += 1;
            }
        }
        if p < len && (input[p].is_ascii_alphabetic() || input[p] == b'.') {
            return None;
        }
        Some(p)
    }

    #[allow(clippy::while_let_loop)]
    fn number(&mut self, start: usize, line: u32, col: u32, space_before: bool) -> Result<Token> {
        if let Some(end) = self.float_end() {
            let raw: Vec<u8> = self.input[start..end]
                .iter()
                .copied()
                .filter(|byte| *byte != b'\'')
                .collect();
            let text = String::from_utf8_lossy(&raw).into_owned();
            match text.parse::<f64>() {
                Ok(value) => {
                    self.advance_to(end);
                    return Ok(self.make_token(
                        TokenKind::Float(value),
                        start,
                        line,
                        col,
                        space_before,
                    ));
                }
                Err(_) => {
                    return Err(error(
                        15,
                        format!("Invalid expression: {}", self.line_snippet(start)),
                        line,
                    ))
                }
            }
        }
        if self.peek(0) == Some(b'0') && matches!(self.peek(1), Some(b'z') | Some(b'Z')) {
            return self.blob(start, line, col, space_before);
        }
        let radix = match self.peek(1) {
            Some(b'x') | Some(b'X') if self.peek(0) == Some(b'0') => Some(16u32),
            Some(b'b') | Some(b'B') if self.peek(0) == Some(b'0') => Some(2u32),
            Some(b'o') | Some(b'O') if self.peek(0) == Some(b'0') => Some(8u32),
            _ => None,
        };
        if let Some(radix) = radix {
            self.advance_by(2);
            let mut value: u128 = 0;
            let mut any = false;
            loop {
                let Some(byte) = self.peek(0) else { break };
                if let Some(digit) = (byte as char).to_digit(radix) {
                    any = true;
                    value = value.saturating_mul(u128::from(radix));
                    value = value.saturating_add(u128::from(digit));
                    self.advance();
                } else if byte == b'\''
                    && any
                    && self
                        .peek(1)
                        .is_some_and(|next| (next as char).is_digit(radix))
                {
                    self.advance();
                } else {
                    break;
                }
            }
            if !any {
                return Err(error(
                    15,
                    format!("Invalid expression: {}", self.line_snippet(start)),
                    line,
                ));
            }
            let value = value.min(i64::MAX as u128) as i64;
            return Ok(self.make_token(TokenKind::Number(value), start, line, col, space_before));
        }
        // Decimal (leading zeros are decimal in Vim9, not octal).
        let mut value: u128 = 0;
        let mut any = false;
        loop {
            let Some(byte) = self.peek(0) else { break };
            if byte.is_ascii_digit() {
                any = true;
                value = value.saturating_mul(10);
                value = value.saturating_add(u128::from(byte - b'0'));
                self.advance();
            } else if byte == b'\'' && any && self.peek(1).is_some_and(|next| next.is_ascii_digit())
            {
                self.advance();
            } else {
                break;
            }
        }
        debug_assert!(any);
        let value = value.min(i64::MAX as u128) as i64;
        Ok(self.make_token(TokenKind::Number(value), start, line, col, space_before))
    }

    #[allow(clippy::while_let_loop)]
    fn blob(&mut self, start: usize, line: u32, col: u32, space_before: bool) -> Result<Token> {
        self.advance_by(2); // "0z"
        let mut bytes = Vec::new();
        loop {
            let Some(first) = self.peek(0) else { break };
            let Some(high) = (first as char).to_digit(16) else {
                break;
            };
            let Some(low) = self.peek(1).and_then(|byte| (byte as char).to_digit(16)) else {
                return Err(error(
                    973,
                    "Blob literal should have an even number of hex characters",
                    line,
                ));
            };
            bytes.push(((high << 4) | low) as u8);
            self.advance_by(2);
            // A '.' may separate pairs of hex digits.
            if self.peek(0) == Some(b'.')
                && self.peek(1).is_some_and(|byte| byte.is_ascii_hexdigit())
            {
                self.advance();
            }
        }
        Ok(self.make_token(TokenKind::Blob(bytes), start, line, col, space_before))
    }

    // -----------------------------------------------------------------
    // Strings
    // -----------------------------------------------------------------

    fn quoted_string(
        &mut self,
        quote: u8,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        self.advance(); // quote
        let mut out: Vec<u8> = Vec::new();
        loop {
            if self.pos >= self.input.len() {
                return Err(self.unterminated_string(quote, start, line));
            }
            if newline_len(self.input, self.pos) > 0 {
                if let Some(target) = self.continuation_target() {
                    self.join_continuation(target);
                    continue;
                }
                return Err(self.unterminated_string(quote, start, line));
            }
            let byte = self.input[self.pos];
            if byte == quote {
                self.advance();
                if quote == b'\'' && self.peek(0) == Some(b'\'') {
                    out.push(b'\'');
                    self.advance();
                    continue;
                }
                break;
            }
            if quote == b'"' && byte == b'\\' {
                self.advance();
                if self.pos >= self.input.len() || newline_len(self.input, self.pos) > 0 {
                    return Err(self.unterminated_string(quote, start, line));
                }
                self.escape_sequence(&mut out)?;
                continue;
            }
            out.push(byte);
            self.advance();
        }
        Ok(self.make_token(
            TokenKind::String(VimString::from(out)),
            start,
            line,
            col,
            space_before,
        ))
    }

    fn unterminated_string(&self, quote: u8, start: usize, line: u32) -> VimError {
        let snippet = self.line_snippet(start);
        if quote == b'"' {
            error(
                codes::E114,
                format!("Missing double quote: {snippet}"),
                line,
            )
        } else {
            error(
                codes::E115,
                format!("Missing single quote: {snippet}"),
                line,
            )
        }
    }

    /// Copy of the legacy VimL backslash escape semantics (see
    /// crate::lexer), which is also what Vim9 uses inside double quoted
    /// strings.  The cursor is just after the backslash.
    fn escape_sequence(&mut self, out: &mut Vec<u8>) -> Result<()> {
        let Some(byte) = self.advance_byte() else {
            return Err(error(codes::E114, "Missing double quote", self.line));
        };
        match byte {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'e' => out.push(0x1b),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'a' => out.push(0x07),
            b'v' => out.push(0x0b),
            b'\\' => out.push(b'\\'),
            b'"' => out.push(b'"'),
            b'0'..=b'7' => {
                let mut value = u32::from(byte - b'0');
                for _ in 0..2 {
                    if let Some(next) = self.peek(0) {
                        if (b'0'..=b'7').contains(&next) {
                            value = value * 8 + u32::from(next - b'0');
                            self.advance();
                        } else {
                            break;
                        }
                    }
                }
                out.push(value as u8);
            }
            b'x' => {
                let mut value: u32 = 0;
                let mut count = 0;
                while count < 2 {
                    if let Some(next) = self.peek(0) {
                        if let Some(digit) = (next as char).to_digit(16) {
                            value = value * 16 + digit;
                            self.advance();
                            count += 1;
                            continue;
                        }
                    }
                    break;
                }
                if count == 0 {
                    // Vim copies the character after an invalid "\x".
                    out.push(b'x');
                    return Ok(());
                }
                out.push(value as u8);
            }
            b'u' | b'U' => {
                let count = if byte == b'u' { 4 } else { 8 };
                let mut value: u32 = 0;
                for _ in 0..count {
                    let Some(next) = self.peek(0) else {
                        return Err(error(codes::E114, "Missing double quote", self.line));
                    };
                    let Some(digit) = (next as char).to_digit(16) else {
                        // Vim drops the backslash and keeps the character.
                        out.push(byte);
                        return Ok(());
                    };
                    value = value * 16 + digit;
                    self.advance();
                }
                if let Some(character) = char::from_u32(value) {
                    let mut buffer = [0u8; 4];
                    out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
                }
            }
            b'<' => {
                let name_start = self.pos;
                while let Some(next) = self.peek(0) {
                    if next == b'>' {
                        break;
                    }
                    self.advance();
                }
                if self.peek(0) != Some(b'>') {
                    out.push(b'<');
                    out.extend_from_slice(&self.input[name_start..self.pos]);
                    return Ok(());
                }
                let name = &self.input[name_start..self.pos];
                self.advance(); // '>'
                match special_key(name) {
                    Some(bytes) => out.extend_from_slice(&bytes),
                    // Vim drops the backslash of an unrecognized "\<name>"
                    // and copies the remaining characters.
                    None => {
                        out.push(b'<');
                        let mut index = 0;
                        while index < name.len() {
                            if name[index] == b'\\' && index + 1 < name.len() {
                                index += 1;
                            }
                            out.push(name[index]);
                            index += 1;
                        }
                        out.push(b'>');
                    }
                }
            }
            other => out.push(other),
        }
        Ok(())
    }

    fn advance_byte(&mut self) -> Option<u8> {
        let byte = self.input.get(self.pos).copied()?;
        self.advance();
        Some(byte)
    }

    // -----------------------------------------------------------------
    // Interpolated strings: $"..." and $'...'
    // -----------------------------------------------------------------

    fn interpolated_string(
        &mut self,
        start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<Token> {
        let quote = self.peek(1).unwrap_or(b'"');
        self.advance_by(2); // "$" and the quote
        let mut parts: Vec<InterpPart> = Vec::new();
        let mut text: Vec<u8> = Vec::new();
        loop {
            if self.pos >= self.input.len() {
                return Err(self.unterminated_string(quote, start, line));
            }
            if newline_len(self.input, self.pos) > 0 {
                if let Some(target) = self.continuation_target() {
                    self.join_continuation(target);
                    continue;
                }
                return Err(self.unterminated_string(quote, start, line));
            }
            let byte = self.input[self.pos];
            if byte == quote {
                self.advance();
                if quote == b'\'' && self.peek(0) == Some(b'\'') {
                    text.push(b'\'');
                    self.advance();
                    continue;
                }
                break;
            }
            if quote == b'"' && byte == b'\\' {
                self.advance();
                if self.pos >= self.input.len() || newline_len(self.input, self.pos) > 0 {
                    return Err(self.unterminated_string(quote, start, line));
                }
                self.escape_sequence(&mut text)?;
                continue;
            }
            if byte == b'{' {
                if self.peek(1) == Some(b'{') {
                    text.push(b'{');
                    self.advance_by(2);
                    continue;
                }
                if !text.is_empty() {
                    parts.push(InterpPart::Text(VimString::from(std::mem::take(&mut text))));
                }
                self.advance(); // '{'
                let expr_line = self.line;
                let source = self.interpolation_expression(expr_line)?;
                parts.push(InterpPart::Expr {
                    source,
                    line: expr_line,
                });
                continue;
            }
            if byte == b'}' {
                if self.peek(1) == Some(b'}') {
                    text.push(b'}');
                    self.advance_by(2);
                    continue;
                }
                return Err(error(1278, "Stray '}' without a matching '{'", self.line));
            }
            text.push(byte);
            self.advance();
        }
        if !text.is_empty() {
            parts.push(InterpPart::Text(VimString::from(text)));
        }
        Ok(self.make_token(
            TokenKind::InterpolatedString(parts),
            start,
            line,
            col,
            space_before,
        ))
    }

    /// Capture the raw source of one "{expr}" inside an interpolated string.
    /// The cursor is just after the opening '{'; on return it is just after
    /// the matching '}'.  Braces are matched while skipping string literals;
    /// the expression itself is not parsed here.
    fn interpolation_expression(&mut self, start_line: u32) -> Result<VimString> {
        let source_start = self.pos;
        let mut depth = 1usize;
        loop {
            if self.pos >= self.input.len() {
                return Err(error(1279, "Missing '}'", start_line));
            }
            if newline_len(self.input, self.pos) > 0 {
                return Err(error(1279, "Missing '}'", start_line));
            }
            let byte = self.input[self.pos];
            match byte {
                b'{' => {
                    depth += 1;
                    self.advance();
                }
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let source = VimString::from(&self.input[source_start..self.pos]);
                        self.advance();
                        return Ok(source);
                    }
                    self.advance();
                }
                b'\'' | b'"' => self.skip_string_for_matching(byte, start_line)?,
                b'$' if matches!(self.peek(1), Some(b'"') | Some(b'\'')) => {
                    self.skip_interpolated_for_matching(start_line)?;
                }
                _ => self.advance(),
            }
        }
    }

    fn skip_string_for_matching(&mut self, quote: u8, start_line: u32) -> Result<()> {
        self.advance(); // opening quote
        loop {
            if self.pos >= self.input.len() || newline_len(self.input, self.pos) > 0 {
                return Err(error(1279, "Missing '}'", start_line));
            }
            let byte = self.input[self.pos];
            if byte == quote {
                self.advance();
                if quote == b'\'' && self.peek(0) == Some(b'\'') {
                    self.advance();
                    continue;
                }
                return Ok(());
            }
            if quote == b'"' && byte == b'\\' {
                self.advance();
                if self.pos >= self.input.len() || newline_len(self.input, self.pos) > 0 {
                    return Err(error(1279, "Missing '}'", start_line));
                }
                if self.peek(0) == Some(b'<') {
                    // "\<C-W>": skip to the '>' so braces inside do not count.
                    while let Some(next) = self.peek(0) {
                        self.advance();
                        if next == b'>' {
                            break;
                        }
                    }
                } else {
                    self.advance();
                }
                continue;
            }
            self.advance();
        }
    }

    fn skip_interpolated_for_matching(&mut self, start_line: u32) -> Result<()> {
        let quote = self.peek(1).unwrap_or(b'"');
        self.advance_by(2); // "$" and the quote
        loop {
            if self.pos >= self.input.len() || newline_len(self.input, self.pos) > 0 {
                return Err(error(1279, "Missing '}'", start_line));
            }
            let byte = self.input[self.pos];
            if byte == quote {
                self.advance();
                if quote == b'\'' && self.peek(0) == Some(b'\'') {
                    self.advance();
                    continue;
                }
                return Ok(());
            }
            if quote == b'"' && byte == b'\\' {
                self.advance();
                if self.pos < self.input.len() {
                    self.advance();
                }
                continue;
            }
            if byte == b'{' && self.peek(1) == Some(b'{') {
                self.advance_by(2);
                continue;
            }
            if byte == b'}' && self.peek(1) == Some(b'}') {
                self.advance_by(2);
                continue;
            }
            if byte == b'{' {
                self.advance();
                self.skip_expression_for_matching(start_line)?;
                continue;
            }
            self.advance();
        }
    }

    fn skip_expression_for_matching(&mut self, start_line: u32) -> Result<()> {
        let mut depth = 1usize;
        loop {
            if self.pos >= self.input.len() || newline_len(self.input, self.pos) > 0 {
                return Err(error(1279, "Missing '}'", start_line));
            }
            let byte = self.input[self.pos];
            match byte {
                b'{' => {
                    depth += 1;
                    self.advance();
                }
                b'}' => {
                    depth -= 1;
                    self.advance();
                    if depth == 0 {
                        return Ok(());
                    }
                }
                b'\'' | b'"' => self.skip_string_for_matching(byte, start_line)?,
                b'$' if matches!(self.peek(1), Some(b'"') | Some(b'\'')) => {
                    self.skip_interpolated_for_matching(start_line)?;
                }
                _ => self.advance(),
            }
        }
    }

    // -----------------------------------------------------------------
    // Heredocs: "var x =<< [trim] [eval] MARKER"
    // -----------------------------------------------------------------

    fn heredoc(&mut self, start: usize, line: u32, col: u32, space_before: bool) -> Result<Token> {
        // Emit Punct::Assign for the '=', queue the Heredoc token and a
        // synthetic Newline for the end of the heredoc.
        self.advance(); // '='
        let assign_token = self.make_token(
            TokenKind::Punct(Punct::Assign),
            start,
            line,
            col,
            space_before,
        );
        let (heredoc_token, newline_token) = self.finish_heredoc(start, line, col, space_before)?;
        self.queue.push_back(heredoc_token);
        self.queue.push_back(newline_token);
        Ok(assign_token)
    }

    fn finish_heredoc(
        &mut self,
        assign_start: usize,
        line: u32,
        col: u32,
        space_before: bool,
    ) -> Result<(Token, Token)> {
        // The physical line that started the assignment: its leading white
        // space is the indentation "trim" compares against the end marker.
        let command_line_start = self.pos.saturating_sub(col as usize);
        let mut command_indent_len = 0;
        while command_line_start + command_indent_len < self.input.len()
            && matches!(
                self.input[command_line_start + command_indent_len],
                b' ' | b'\t'
            )
        {
            command_indent_len += 1;
        }
        let mut index = self.pos + 2; // after "=<<" (self.pos is at '<')
        let line_end = self.line_end(index);
        let mut flags = HeredocFlags::default();
        loop {
            while index < line_end && matches!(self.input[index], b' ' | b'\t') {
                index += 1;
            }
            let word_end = index + 4;
            let boundary = word_end >= line_end
                || matches!(self.input[word_end], b' ' | b'\t')
                || self.input[word_end] == b'#';
            if word_end <= line_end && boundary {
                let word = &self.input[index..word_end.min(line_end)];
                if word == b"trim" {
                    flags.trim = true;
                    index += 4;
                    continue;
                }
                if word == b"eval" {
                    flags.eval = true;
                    index += 4;
                    continue;
                }
            }
            break;
        }
        while index < line_end && matches!(self.input[index], b' ' | b'\t') {
            index += 1;
        }
        if index >= line_end || self.input[index] == b'#' {
            return Err(error(172, "Missing marker", line));
        }
        let marker_start = index;
        while index < line_end && !matches!(self.input[index], b' ' | b'\t') {
            index += 1;
        }
        let marker = self.input[marker_start..index].to_vec();
        if marker.first().is_some_and(u8::is_ascii_lowercase) {
            return Err(error(
                221,
                "Marker cannot start with lower case letter",
                line,
            ));
        }
        // Only white space or a comment may follow the marker.
        let mut after = index;
        while after < line_end && matches!(self.input[after], b' ' | b'\t') {
            after += 1;
        }
        if after < line_end && self.input[after] != b'#' {
            return Err(error(
                488,
                format!("Trailing characters: {}", self.line_snippet(after)),
                line,
            ));
        }
        // Move past the assignment line.
        let mut cursor = line_end;
        let mut cursor_line = line;
        if newline_len(self.input, cursor) == 0 {
            return Err(error(
                990,
                format!("Missing end marker '{}'", String::from_utf8_lossy(&marker)),
                line,
            ));
        }
        cursor += newline_len(self.input, cursor);
        cursor_line += 1;
        let mut lines: Vec<VimString> = Vec::new();
        let indent = &self.input[command_line_start..command_line_start + command_indent_len];
        loop {
            if cursor >= self.input.len() {
                return Err(error(
                    990,
                    format!("Missing end marker '{}'", String::from_utf8_lossy(&marker)),
                    line,
                ));
            }
            let content_start = cursor;
            let mut content_end = cursor;
            while content_end < self.input.len() && newline_len(self.input, content_end) == 0 {
                content_end += 1;
            }
            let content = &self.input[content_start..content_end];
            let mut marker_indent = 0;
            if flags.trim
                && !indent.is_empty()
                && content.len() >= indent.len()
                && &content[..indent.len()] == indent
            {
                marker_indent = indent.len();
            }
            if &content[marker_indent..] == marker.as_slice() {
                let terminator_line = cursor_line;
                let terminator_end = content_end;
                let newline = newline_len(self.input, terminator_end);
                let heredoc_token = Token {
                    kind: TokenKind::Heredoc(Heredoc { flags, lines }),
                    start: assign_start,
                    end: terminator_end,
                    line,
                    col,
                    space_before,
                };
                let newline_token = Token {
                    kind: TokenKind::Newline,
                    start: terminator_end,
                    end: terminator_end + newline,
                    line: terminator_line,
                    col: (terminator_end - content_start + 1) as u32,
                    space_before: false,
                };
                self.pos = terminator_end + newline;
                self.line = terminator_line + 1;
                self.col = 1;
                self.line_has_token = false;
                return Ok((heredoc_token, newline_token));
            }
            lines.push(VimString::from(content));
            let newline = newline_len(self.input, content_end);
            cursor = content_end + newline;
            cursor_line += 1;
        }
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'#'
}

fn special_key(name: &[u8]) -> Option<Vec<u8>> {
    let upper = name.to_ascii_uppercase();
    if upper.len() >= 3 && upper.starts_with(b"C-") {
        let key = upper[2];
        let byte = match key {
            b'?' => 0x7f,
            b'@' => 0,
            b'[' => 0x1b,
            b'\\' => 0x1c,
            b']' => 0x1d,
            b'^' => 0x1e,
            b'_' => 0x1f,
            b'A'..=b'Z' => key & 0x1f,
            _ => return None,
        };
        return Some(vec![byte]);
    }
    match upper.as_slice() {
        b"CR" => Some(vec![13]),
        b"NL" | b"LF" => Some(vec![10]),
        b"TAB" => Some(vec![9]),
        b"ESC" => Some(vec![27]),
        b"SPACE" => Some(vec![32]),
        b"BS" => Some(vec![8]),
        b"DEL" => Some(vec![127]),
        b"NUL" => Some(vec![0]),
        _ => None,
    }
}
