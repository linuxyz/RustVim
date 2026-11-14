//! Ex command parser: ranges, addresses, modifiers and command arguments.
//!
//! The parser does not execute commands. It produces an AST that the editor
//! resolves against its own buffer, marks and search state.

pub mod ast;
pub mod parser;

pub use ast::{Address, AddressKind, ExCommand, Modifier, RangeSep, RangeSpec, ResolvedRange};
pub use parser::{parse, ExParseError};
