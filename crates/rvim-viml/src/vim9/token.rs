//! Token definitions for the Vim9 script front end.
//!
//! Vim9 is line oriented: statements are separated by newlines, and a newline
//! inside brackets or around a binary operator continues the expression.  The
//! lexer therefore emits TokenKind::Newline tokens; the parser decides where a
//! newline is a continuation (see the Vim9 "line continuation" docs).
//!
//! Keywords are not separate token kinds.  Vim9 reserves a number of words but
//! they are only keywords in specific positions.  Keeping them as identifiers
//! makes the parser contextual and the error messages Vim-like.

use rvim_types::VimString;

use crate::lexer::Scope;

/// One piece of an interpolated string (dollar-quoted strings).
#[derive(Clone, Debug, PartialEq)]
pub enum InterpPart {
    /// Literal decoded text.
    Text(VimString),
    /// Raw source of an embedded {expr}, with the line it starts on.
    Expr { source: VimString, line: u32 },
}

/// Flags that may precede a heredoc marker: "=&lt;&lt; trim eval END".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeredocFlags {
    /// Strip leading white space from every content line.
    pub trim: bool,
    /// Evaluate every content line as a Vim9 expression.
    pub eval: bool,
}

/// A "=&lt;&lt; \[trim\] \[eval\] MARKER" heredoc literal.
#[derive(Clone, Debug, PartialEq)]
pub struct Heredoc {
    pub flags: HeredocFlags,
    /// Raw content lines, before any "trim" processing.
    pub lines: Vec<VimString>,
}

/// Punctuation and operators.  Longest match wins in the lexer, so ".." is a
/// single token and never two "." tokens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Punct {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// Member access: .name
    Dot,
    /// String concatenation: ..
    DotDot,
    /// Method call: receiver-&gt;method()
    Arrow,
    /// Lambda arrow: (args) =&gt; expr
    FatArrow,
    /// Assignment "=" (only valid in assignment/declaration position).
    Assign,
    EqEq,
    NotEq,
    Gt,
    Ge,
    Lt,
    Le,
    /// "=~"
    Match,
    /// "!~"
    NotMatch,
    Bang,
    Question,
    /// Falsy operator "??".
    QuestionQuestion,
    Colon,
    Semicolon,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    PlusAssign,
    MinusAssign,
    StarAssign,
    SlashAssign,
    PercentAssign,
    DotDotAssign,
    AndAnd,
    OrOr,
    /// "|" (Ex command separator) only appears in raw command lines; it is
    /// tokenized so the parser can recognize command continuations.
    Bar,
    /// "..."
    Ellipsis,
}

/// One lexical token.
#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    Number(i64),
    Float(f64),
    /// Decoded string value (escapes processed, doubled single quotes
    /// collapsed).
    String(VimString),
    /// Dollar-quoted interpolated string.
    InterpolatedString(Vec<InterpPart>),
    Blob(Vec<u8>),
    Heredoc(Heredoc),
    /// A plain name: foo, MyFunc, foo#bar#Baz.
    Ident(VimString),
    /// g:foo, b:foo, v:true, ... (s: only in legacy context).
    ScopedVar(Scope, VimString),
    /// &option / &l:option / &g:option; the name keeps the scope prefix.
    Option(String),
    /// $ENV
    Env(String),
    /// @a
    Register(u8),
    Punct(Punct),
    /// End of a physical line (statement separator unless continued).
    Newline,
    Eof,
}

impl TokenKind {
    /// The identifier text, when this token is a plain name.
    pub fn ident(&self) -> Option<&VimString> {
        match self {
            TokenKind::Ident(name) => Some(name),
            _ => None,
        }
    }

    /// True when this token is the identifier "word".
    pub fn is_ident(&self, word: &str) -> bool {
        matches!(self, TokenKind::Ident(name) if name.as_bytes() == word.as_bytes())
    }

    pub fn is_punct(&self, punct: Punct) -> bool {
        matches!(self, TokenKind::Punct(found) if *found == punct)
    }
}

/// A token plus its source location.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    /// Byte offset of the first byte of the token.
    pub start: usize,
    /// Byte offset one past the last byte of the token.
    pub end: usize,
    /// 1-based physical line.
    pub line: u32,
    /// 1-based byte column.
    pub col: u32,
    /// Whether white space preceded the token on the same line.
    pub space_before: bool,
}

impl Token {
    pub fn kind(&self) -> &TokenKind {
        &self.kind
    }
}

/// Statement keywords that start a Vim9 language construct.  These cannot be
/// used to start an Ex command line.
pub const STATEMENT_KEYWORDS: &[&str] = &[
    "vim9script",
    "def",
    "enddef",
    "var",
    "final",
    "const",
    "if",
    "elseif",
    "else",
    "endif",
    "while",
    "endwhile",
    "for",
    "endfor",
    "try",
    "catch",
    "finally",
    "endtry",
    "return",
    "throw",
    "break",
    "continue",
    "defer",
    "class",
    "endclass",
    "interface",
    "endinterface",
    "enum",
    "endenum",
    "type",
    "import",
    "export",
    "abstract",
    "public",
    "private",
    "static",
];

/// Words that terminate an enclosing block.
pub const BLOCK_TERMINATORS: &[&str] = &[
    "enddef",
    "elseif",
    "else",
    "endif",
    "endwhile",
    "endfor",
    "catch",
    "finally",
    "endtry",
    "endclass",
    "endinterface",
    "endenum",
];

/// True when "word" starts a Vim9 statement.
pub fn is_statement_keyword(word: &[u8]) -> bool {
    STATEMENT_KEYWORDS
        .iter()
        .any(|keyword| keyword.as_bytes() == word)
}

/// True when "word" terminates a block.
pub fn is_block_terminator(word: &[u8]) -> bool {
    BLOCK_TERMINATORS
        .iter()
        .any(|keyword| keyword.as_bytes() == word)
}
