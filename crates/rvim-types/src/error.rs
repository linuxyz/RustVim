use std::fmt;

/// A Vim-style error. The code is kept separate from the message because
/// scripts, tests and the message pipeline all need the numeric code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VimError {
    pub code: Option<u32>,
    pub message: String,
    pub throwpoint: Option<String>,
}

pub type Result<T> = std::result::Result<T, VimError>;

impl VimError {
    pub fn new(code: u32, message: impl Into<String>) -> Self {
        Self {
            code: Some(code),
            message: message.into(),
            throwpoint: None,
        }
    }

    pub fn plain(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
            throwpoint: None,
        }
    }

    pub fn with_throwpoint(mut self, point: impl Into<String>) -> Self {
        self.throwpoint = Some(point.into());
        self
    }

    pub fn interrupt() -> Self {
        Self::plain("Interrupted")
    }
}

impl fmt::Display for VimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.code {
            Some(code) => write!(f, "E{:03}: {}", code, self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for VimError {}

impl From<std::io::Error> for VimError {
    fn from(err: std::io::Error) -> Self {
        Self::plain(err.to_string())
    }
}
