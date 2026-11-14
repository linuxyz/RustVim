//! Vim9 script front end: lexer, AST and parser.
//!
//! This module is deliberately independent of the legacy VimL interpreter.
//! Phase 1 provides the tokenizer, AST and parser; later phases add static
//! type checking, an evaluator and the script loader integration.  Nothing in
//! here is reachable from normal Vim script execution until the
//! "has('vim9script')" flip in a later phase.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod token;

#[cfg(test)]
mod tests_lexer;

#[cfg(test)]
mod tests_parser;
