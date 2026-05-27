#![cfg_attr(not(feature = "fs"), no_std)]

extern crate alloc;

mod parser;
mod matcher;
mod operator;
mod globber;
#[cfg(feature = "fs")]
mod fs;

pub use parser::{compile_pattern, MatchToken, ParseError, ParseErrorKind, ParsedPattern};
pub use matcher::wildcard_match;
pub use operator::{FnOperator, GlobOperator, ReverseOp};
pub use globber::{Globber, GlobberBuilder, ScanHint};
