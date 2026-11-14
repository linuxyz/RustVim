//! Error types reported by the regex compiler and matcher.

use std::error::Error;
use std::fmt;

/// A pattern compilation error.
///
/// `offset` is the byte offset in the pattern at which the problem was
/// detected.  It may be equal to the pattern length for errors such as an
/// unterminated group or collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexError {
    /// Human readable description, including a Vim-like error code where one
    /// exists.
    pub message: String,
    /// Byte offset in the original pattern.
    pub offset: usize,
}

impl RegexError {
    pub(crate) fn new(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
        }
    }
}

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "regex error at byte {}: {}", self.offset, self.message)
    }
}

impl Error for RegexError {}

/// A match-time failure.
///
/// The only failure currently possible is running out of the configured
/// backtracking step budget.  A match that exceeds the budget stops cleanly;
/// it never loops forever.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchError {
    /// The matcher used `limit` steps without finding a result.
    StepLimitExceeded {
        /// The configured step budget for the call.
        limit: u64,
    },
}

impl fmt::Display for MatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StepLimitExceeded { limit } => {
                write!(f, "regex step limit of {limit} exceeded")
            }
        }
    }
}

impl Error for MatchError {}
