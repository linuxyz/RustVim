//! Regular-expression abstract syntax tree.
//!
//! The parser produces this tree; the compiler lowers it into the backtracking
//! VM program.  Keeping the tree separate makes precedence, magic modes and
//! diagnostics much easier to reason about.

use crate::charclass::CharClass;

/// Zero-width boundary assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    /// `^`, \_`^\` and the start of a line.
    LineStart,
    /// `$`, \_`$\` and the end of a line.
    LineEnd,
    /// `\<`.
    WordStart,
    /// `\>`.
    WordEnd,
    /// `\%^`.
    TextStart,
    /// `\%$`.
    TextEnd,
    /// An atom that needs editor context this engine does not have
    /// (`\%#`, `\%V`, line/column/mark atoms).  It never matches.
    Never,
}

/// Kinds of look-around assertions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LookKind {
    /// `\@=`: zero width, the inner pattern must match here.
    Ahead,
    /// `\@!`: zero width, the inner pattern must not match here.
    NotAhead,
    /// `\@<=`: zero width, the inner pattern must match ending here.
    Behind,
    /// `\@<!`: zero width, the inner pattern must not match ending here.
    NotBehind,
}

/// Parsed pattern node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Ast {
    /// Matches the empty string.  Used for magic switches and empty branches.
    Empty,
    /// One literal byte.
    Literal(u8),
    /// `.`: one byte, but not a line break.
    Any,
    /// `\_.`: one byte including a line break.
    AnyNewline,
    /// A collection or a class atom.
    Class(CharClass),
    /// A zero-width boundary assertion.
    Anchor(Anchor),
    /// Concatenation.
    Concat(Vec<Ast>),
    /// Alternation.  Earlier branches are preferred.
    Alt(Vec<Ast>),
    /// A capturing group.  `index` is one based.
    Group { index: usize, node: Box<Ast> },
    /// A greedy or lazy repetition.
    Repeat {
        node: Box<Ast>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
    /// `\1` .. `\9`.
    Backref(usize),
    /// `\zs`.
    SetStart,
    /// `\ze`.
    SetEnd,
    /// A look-around assertion around one atom.
    Look {
        kind: LookKind,
        node: Box<Ast>,
        /// Optional `\@123<=` byte limit.
        limit: Option<usize>,
    },
    /// `\@>`: match the atom once and never backtrack into it.
    Atomic(Box<Ast>),
}
