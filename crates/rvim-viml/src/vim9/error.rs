//! Error helpers for the Vim9 front end.
//!
//! Vim9 compile errors use the same E-codes as Vim.  The parser reports the
//! first error and stops, like Vim's compiler; the enclosing source line is
//! attached to the message so script-sourcing code can present it.

use crate::error::{Result, VimError};

/// Create a Vim9 compile error for "line".
pub fn error(code: u32, message: impl Into<String>, line: u32) -> VimError {
    let message = message.into();
    if line == 0 {
        return VimError::new(code, message);
    }
    VimError::new(code, format!("{message} (line {line})"))
}

/// Result alias used by the Vim9 front end.
pub type Vim9Result<T> = Result<T>;

/// Vim9 error codes used by the parser.  Only a subset is needed so far; more
/// are added as the parser grows toward full Vim9 coverage.
pub mod codes {
    /// Expected a name.
    pub const E1004: u32 = 1004;
    /// Missing ')' after expression.
    pub const E110: u32 = 110;
    /// Missing '\]' after expression.
    pub const E111: u32 = 111;
    /// Missing '}' after expression.
    pub const E116: u32 = 116;
    /// Trailing characters after expression.
    pub const E488: u32 = 488;
    /// Line incomplete.
    pub const E1097: u32 = 1097;
    /// Expected :enddef.
    pub const E1098: u32 = 1098;
    /// Expected :endif.
    pub const E580: u32 = 580;
    /// Expected :endwhile.
    pub const E581: u32 = 581;
    /// Expected :endfor.
    pub const E582: u32 = 582;
    /// Expected :endtry.
    pub const E584: u32 = 584;
    /// :try without :catch or :finally.
    pub const E1032: u32 = 1032;
    /// Expected :endclass.
    pub const E1065: u32 = 1065;
    /// Expected :endinterface.
    pub const E1094: u32 = 1094;
    /// Expected :endenum.
    pub const E1095: u32 = 1095;
    /// Using a reserved name.
    pub const E1030: u32 = 1030;
    /// Duplicate function argument.
    pub const E853: u32 = 853;
    /// Missing end of line.
    pub const E1137: u32 = 1137;
    /// Cannot use :var outside a function or script.
    pub const E1126: u32 = 1126;
    /// Number too large.
    pub const E951: u32 = 951;
    /// Unterminated string.
    pub const E114: u32 = 114;
    /// Unterminated single-quoted string.
    pub const E115: u32 = 115;
}
