//! Vim-compatible errors, throwpoints and exception matching.

use std::fmt;

/// Distinguishes a Vim script `:throw` exception from an interpreter error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    /// A regular Vim error (E-codes). The exception string is `Vim(...):E...`.
    Error,
    /// A value raised with `:throw`. The exception string is the thrown value.
    Throw,
}

/// A Vim error with an optional E-code, message and throwpoint.
///
/// This mirrors `rvim_types::VimError` but adds exception identity needed by
/// `:try`/`:catch` and supports lossless conversion in both directions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VimError {
    pub code: Option<u32>,
    pub message: String,
    pub throwpoint: Option<String>,
    /// The string matched by `:catch` patterns and exposed as `v:exception`.
    pub exception: Option<String>,
    pub kind: ErrorKind,
}

pub type Result<T> = std::result::Result<T, VimError>;

impl VimError {
    pub fn new(code: u32, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            message: message.into(),
            throwpoint: None,
            exception: None,
            kind: ErrorKind::Error,
        }
    }

    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            throwpoint: None,
            exception: None,
            kind: ErrorKind::Error,
        }
    }

    /// Create an exception raised by `:throw {value}`.
    pub fn thrown(exception: impl Into<String>) -> Self {
        let exception = exception.into();
        Self {
            code: None,
            message: exception.clone(),
            throwpoint: None,
            exception: Some(exception),
            kind: ErrorKind::Throw,
        }
    }

    pub fn with_throwpoint(mut self, point: impl Into<String>) -> Self {
        if self.throwpoint.is_none() {
            self.throwpoint = Some(point.into());
        }
        self
    }

    pub fn with_exception(mut self, exception: impl Into<String>) -> Self {
        if self.exception.is_none() {
            self.exception = Some(exception.into());
        }
        self
    }

    pub fn is_throw(&self) -> bool {
        self.kind == ErrorKind::Throw
    }

    /// Convert an uncaught `:throw` into Vim's E605 error.
    pub fn into_uncaught_throw(mut self) -> Self {
        if self.kind == ErrorKind::Throw {
            let value = self
                .exception
                .clone()
                .unwrap_or_else(|| self.message.clone());
            self.exception = Some(value.clone());
            self.code = Some(605);
            self.message = format!("Exception not caught: {value}");
            self.kind = ErrorKind::Error;
        }
        self
    }

    pub fn interrupt() -> Self {
        Self::plain("Interrupted")
    }

    /// The `E123: message` form, without a throwpoint.
    pub fn error_text(&self) -> String {
        match self.code {
            Some(code) => format!("E{code:03}: {}", self.message),
            None => self.message.clone(),
        }
    }

    /// The value exposed as `v:exception`.
    ///
    /// Errors use Vim's `Vim(cmd):E123: ...` form; thrown values are used
    /// verbatim.
    pub fn exception_string(&self) -> String {
        if let Some(exception) = &self.exception {
            return exception.clone();
        }
        match self.kind {
            ErrorKind::Throw => self.message.clone(),
            ErrorKind::Error => format!("Vim:{}", self.error_text()),
        }
    }

    /// Add the Vim command context used in the exception string of an error.
    pub fn with_command(mut self, command: &str) -> Self {
        if self.kind == ErrorKind::Error && self.exception.is_none() {
            self.exception = Some(format!("Vim({command}):{}", self.error_text()));
        }
        self
    }
}

impl fmt::Display for VimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(code) => write!(f, "E{code:03}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for VimError {}

impl From<rvim_types::VimError> for VimError {
    fn from(value: rvim_types::VimError) -> Self {
        let message = value.message;
        let code = value.code;
        Self {
            code,
            message,
            throwpoint: value.throwpoint,
            exception: None,
            kind: ErrorKind::Error,
        }
    }
}

impl From<VimError> for rvim_types::VimError {
    fn from(value: VimError) -> Self {
        Self {
            code: value.code,
            message: value.message,
            throwpoint: value.throwpoint,
        }
    }
}

impl From<std::io::Error> for VimError {
    fn from(value: std::io::Error) -> Self {
        Self::plain(value.to_string())
    }
}

/// Convenience constructors for the error codes the interpreter uses.
pub(crate) mod codes {
    use super::VimError;

    pub fn e15(detail: &str) -> VimError {
        VimError::new(15, format!("Invalid expression: {detail}"))
    }

    pub fn e108(name: &str) -> VimError {
        VimError::new(108, format!("No such variable: {name}"))
    }

    pub fn e117(name: &str) -> VimError {
        VimError::new(117, format!("Unknown function: {name}"))
    }

    pub fn e118(name: &str) -> VimError {
        VimError::new(118, format!("Too many arguments for function: {name}"))
    }

    pub fn e119(name: &str) -> VimError {
        VimError::new(119, format!("Not enough arguments for function: {name}"))
    }

    pub fn e121(name: &str) -> VimError {
        VimError::new(121, format!("Undefined variable: {name}"))
    }

    pub fn e122(name: &str) -> VimError {
        VimError::new(122, format!("Function already exists: {name}"))
    }

    pub fn e128(name: &str) -> VimError {
        VimError::new(
            128,
            format!("Function name must start with a capital or contain a colon: {name}"),
        )
    }

    pub fn e130(name: &str) -> VimError {
        VimError::new(130, format!("Function not found: {name}"))
    }

    pub fn e46(name: &str) -> VimError {
        VimError::new(46, format!("Cannot change read-only variable {name}"))
    }

    pub fn e488(detail: &str) -> VimError {
        VimError::new(488, format!("Trailing characters: {detail}"))
    }

    pub fn e492(command: &str) -> VimError {
        VimError::new(492, format!("Not an editor command: {command}"))
    }

    pub fn e684(index: i64) -> VimError {
        VimError::new(684, format!("List index out of range: {index}"))
    }

    pub fn e716(key: &str) -> VimError {
        VimError::new(716, format!("Key not present in Dictionary: \"{key}\""))
    }

    pub fn e741(name: &str) -> VimError {
        VimError::new(741, format!("Variable is locked: {name}"))
    }
}
