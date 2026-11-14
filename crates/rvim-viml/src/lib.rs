//! Legacy Vim script (VimL) interpreter.
//!
//! Values are byte-string based; scripts are parsed into a statement tree and
//! executed against [`VimRuntime`] plus an editor-provided [`VimHost`].

pub mod builtins;
pub mod error;
pub mod eval;
pub mod gc;
pub mod host;
pub mod lexer;
pub mod ops;
pub mod parser;
pub mod pattern;
pub mod runtime;
pub mod script;
pub mod value;
pub mod vim9;

pub use error::{Result, VimError};
pub use eval::{call_function, call_value, eval_expression};
pub use gc::{collect_cycles, live_object_count};
pub use host::{BasicHost, ExContext, OptionScope, VarScope, VimHost};
pub use runtime::VimRuntime;
pub use rvim_types::VimString;
pub use script::{execute_script, execute_script_in_context, parse_script};
pub use value::{
    SpecialValue, Value, VimDict, VimList, VIM_T_BLOB, VIM_T_BOOL, VIM_T_DICT, VIM_T_FLOAT,
    VIM_T_FUNC, VIM_T_LIST, VIM_T_NONE, VIM_T_NUMBER, VIM_T_STRING,
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_extra;
