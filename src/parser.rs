// Converts raw pattern strings into the token and AST representations used by
// the rest of the crate.  A pattern is first compiled into a flat Vec<MatchToken>
// by compile_pattern, then optionally wrapped in a ParsedPattern tree by
// ParsedPattern::parse when operator nesting (e.g. REVERSE(...)) is present.
// All error types live here because they describe failures that originate during
// parsing and compilation.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::ops::Range;

/// Characters that may follow a backslash escape in a flexiglob pattern.
/// Everything else is either a path separator (use `/`) or needs no escaping.
const GLOB_ESCAPABLE: &[char] = &['*', '?', '[', ']', '(', ')', '"', '\\'];

/// The Abstract Syntax Tree (AST) representing a parsed flexiglob pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedPattern {
    /// A leaf string containing wildcards (e.g. ".text*").
    Leaf {
        pattern: String,
        tokens: Vec<MatchToken>,
    },

    /// A pipeline operator wrapper (e.g. "REVERSE", "SORT").
    Operator {
        name: String,
        inner: Box<ParsedPattern>,
    },
}

/// Token representations compiled from a wildcard pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchToken {
    /// A literal character.
    Char(char),

    /// Matches exactly one character except path separators (represented by '?').
    AnyChar,

    /// Matches zero or more characters including path separators (represented by '**').
    AnySeq,

    /// Matches zero or more characters except path separators (represented by '*').
    AnySeqNoSeparator,

    /// Matches any character in the set (represented by '[chars]').
    Set(BTreeSet<char>),

    /// Matches any non-separator character not in the set (represented by '[^chars]').
    NegatedSet(BTreeSet<char>),
}

/// The syntax error kinds returned during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    EmptyPattern,
    InvalidOperator(String),
    /// An operator with this name was already registered in the builder.
    DuplicateOperator(String),
    MismatchedParentheses,
    UnexpectedParen,
    UnterminatedBracketSet,
    EmptyBrackets,
    UnexpectedTrailingCharacters,
    /// The character after a backslash is not a recognized escapable character.
    InvalidEscape(char),
}

/// Syntax error information pointing to the exact locations inside the pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The type of syntax error that occurred.
    pub kind: ParseErrorKind,

    /// The span of the offending characters relative to the input string start.
    pub span: Range<usize>,

    /// An explanation of the error.
    pub message: String,
}

impl core::fmt::Display for ParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{} at {:?}", self.message, self.span)
    }
}

impl core::error::Error for ParseError {}

/// Compiles a raw wildcard pattern string into compiled match tokens.
pub fn compile_pattern(pattern: &str) -> Result<Vec<MatchToken>, ParseError> {
    let mut tokens = Vec::new();
    // char_indices() pairs give (byte_offset, char), keeping spans in bytes throughout.
    let chars: Vec<(usize, char)> = pattern.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        match ch {
            '\\' => {
                if i + 1 < chars.len() {
                    let (next_byte_pos, next_ch) = chars[i + 1];
                    if !GLOB_ESCAPABLE.contains(&next_ch) {
                        let message = if next_ch.is_alphanumeric() || matches!(next_ch, '.' | '_') {
                            "Backslash is the escape character; use '/' as the path separator in flexiglob patterns".to_string()
                        } else {
                            format!("'\\{}' is not a recognized escape sequence", next_ch)
                        };
                        return Err(ParseError {
                            kind: ParseErrorKind::InvalidEscape(next_ch),
                            span: byte_pos..next_byte_pos + next_ch.len_utf8(),
                            message,
                        });
                    }
                    tokens.push(MatchToken::Char(next_ch));
                    // +2 skips the escaped character; +1 would re-examine it
                    // as a potential wildcard on the next iteration.
                    i += 2;
                } else {
                    return Err(ParseError {
                        kind: ParseErrorKind::UnexpectedTrailingCharacters,
                        span: byte_pos..byte_pos + 1,
                        message: "Dangling backslash at end of pattern".to_string(),
                    });
                }
            }
            '?' => {
                tokens.push(MatchToken::AnyChar);
                i += 1;
            }
            '*' => {
                if i + 1 < chars.len() && chars[i + 1].1 == '*' {
                    tokens.push(MatchToken::AnySeq);
                    i += 2;
                } else {
                    tokens.push(MatchToken::AnySeqNoSeparator);
                    i += 1;
                }
            }
            '[' => {
                let start_byte = byte_pos;
                i += 1; // skip '['
                let negated = i < chars.len() && chars[i].1 == '^';
                if negated { i += 1; } // skip '^'
                let mut set = BTreeSet::new();
                let mut closed = false;
                let mut close_byte = 0;
                // The unterminated-bracket error is checked after this loop, not
                // inside it, so the span can cover the full unclosed fragment.
                while i < chars.len() {
                    let cur_ch = chars[i].1;
                    if cur_ch == ']' {
                        closed = true;
                        close_byte = chars[i].0 + 1; // byte just past ']'
                        i += 1;
                        break;
                    }
                    if cur_ch == '\\' {
                        if i + 1 < chars.len() {
                            let (next_byte_pos, next_ch) = chars[i + 1];
                            if !GLOB_ESCAPABLE.contains(&next_ch) {
                                let message = if next_ch.is_alphanumeric() || matches!(next_ch, '.' | '_') {
                                    "Backslash is the escape character; use '/' as the path separator in flexiglob patterns".to_string()
                                } else {
                                    format!("'\\{}' is not a recognized escape sequence", next_ch)
                                };
                                return Err(ParseError {
                                    kind: ParseErrorKind::InvalidEscape(next_ch),
                                    span: chars[i].0..next_byte_pos + next_ch.len_utf8(),
                                    message,
                                });
                            }
                            set.insert(next_ch);
                            i += 2;
                        } else {
                            return Err(ParseError {
                                kind: ParseErrorKind::UnterminatedBracketSet,
                                span: start_byte..pattern.len(),
                                message: "Dangling backslash inside bracket set".to_string(),
                            });
                        }
                    } else if i + 2 < chars.len() && chars[i + 1].1 == '-' {
                        let start_ch = cur_ch;
                        let end_ch = chars[i + 2].1;
                        if start_ch <= end_ch {
                            for c in (start_ch as u32)..=(end_ch as u32) {
                                if let Some(ch) = char::from_u32(c) {
                                    set.insert(ch);
                                }
                            }
                        } else {
                            // Invalid range (start > end): treat the three characters
                            // as literals, matching the behavior of most glob tools.
                            set.insert(start_ch);
                            set.insert('-');
                            set.insert(end_ch);
                        }
                        i += 3;
                    } else {
                        set.insert(cur_ch);
                        i += 1;
                    }
                }

                if !closed {
                    return Err(ParseError {
                        kind: ParseErrorKind::UnterminatedBracketSet,
                        span: start_byte..pattern.len(),
                        message: "Unterminated bracket set".to_string(),
                    });
                }
                if set.is_empty() {
                    return Err(ParseError {
                        kind: ParseErrorKind::EmptyBrackets,
                        span: start_byte..close_byte,
                        message: "Empty brackets '[]' never match anything".to_string(),
                    });
                }
                if negated {
                    tokens.push(MatchToken::NegatedSet(set));
                } else {
                    tokens.push(MatchToken::Set(set));
                }
            }
            '(' | ')' => {
                // Unescaped parens are reserved for the operator layer (ParsedPattern::parse).
                // Treating them as literals here would silently accept malformed patterns.
                return Err(ParseError {
                    kind: ParseErrorKind::UnexpectedParen,
                    span: byte_pos..byte_pos + 1,
                    message: format!("Unescaped '{}' in glob pattern; use \\{} for a literal parenthesis", ch, ch),
                });
            }
            c => {
                tokens.push(MatchToken::Char(c));
                i += 1;
            }
        }
    }

    Ok(tokens)
}

fn parse_operator_syntax_or_error(
    trimmed: &str,
    current_offset: usize,
) -> Result<Option<(String, String, Range<usize>)>, ParseError> {
    let Some(first_paren) = trimmed.find('(') else {
        return Ok(None);
    };
    let op_name = trimmed[..first_paren].trim();

    // Validate op_name is a valid identifier; '(' is reserved so a non-identifier
    // prefix is always an error rather than a fallthrough to leaf.
    let paren_offset = current_offset + first_paren;
    if op_name.is_empty() {
        return Err(ParseError {
            kind: ParseErrorKind::UnexpectedParen,
            span: paren_offset..paren_offset + 1,
            message: "Unexpected '(' with no operator name".to_string(),
        });
    }
    for (idx, c) in op_name.chars().enumerate() {
        if idx == 0 {
            if !c.is_alphabetic() && c != '_' {
                return Err(ParseError {
                    kind: ParseErrorKind::UnexpectedParen,
                    span: paren_offset..paren_offset + 1,
                    message: format!("'(' must be preceded by a valid operator name, not '{}'", op_name),
                });
            }
        } else {
            if !c.is_alphanumeric() && c != '_' {
                return Err(ParseError {
                    kind: ParseErrorKind::UnexpectedParen,
                    span: paren_offset..paren_offset + 1,
                    message: format!("'(' must be preceded by a valid operator name, not '{}'", op_name),
                });
            }
        }
    }

    // Since we matched `IDENTIFIER(`, we commit to parsing this as an operator command.
    // char_indices() gives (byte_offset, char) so all positions stay in byte coordinates.
    let mut depth = 0usize;
    let mut close_paren_byte: Option<usize> = None;
    for (byte_pos, c) in trimmed.char_indices() {
        if byte_pos < first_paren {
            continue;
        }
        if c == '(' {
            depth += 1;
        } else if c == ')' {
            depth -= 1;
            if depth == 0 {
                close_paren_byte = Some(byte_pos);
                break;
            }
        }
    }

    let close_byte = match close_paren_byte {
        Some(pos) => pos,
        None => {
            let paren_offset = current_offset + first_paren;
            return Err(ParseError {
                kind: ParseErrorKind::MismatchedParentheses,
                span: paren_offset..paren_offset + 1,
                message: "Missing closing parenthesis for operator".to_string(),
            });
        }
    };

    // The closing parenthesis must be the last character of the trimmed string.
    // ')' is ASCII so close_byte + 1 is the byte position immediately after it.
    if close_byte + 1 != trimmed.len() {
        let trailing_start = current_offset + close_byte + 1;
        return Err(ParseError {
            kind: ParseErrorKind::UnexpectedTrailingCharacters,
            span: trailing_start..current_offset + trimmed.len(),
            message: format!("Unexpected trailing characters after operator closing parenthesis: '{}'", &trimmed[close_byte + 1..]),
        });
    }

    let inner_str = trimmed[first_paren + 1..close_byte].to_string();
    let op_name_start = current_offset + trimmed.find(op_name).unwrap();
    let op_span = op_name_start..op_name_start + op_name.len();
    Ok(Some((op_name.to_string(), inner_str, op_span)))
}

impl ParsedPattern {
    /// Parses a pattern string recursively, validating operator names.
    pub fn parse(input: &str, is_valid_op: impl Fn(&str) -> bool) -> Result<Self, ParseError> {
        Self::parse_inner(input, 0, &is_valid_op)
    }

    fn parse_inner(
        input: &str,
        base_offset: usize,
        is_valid_op: &impl Fn(&str) -> bool,
    ) -> Result<Self, ParseError> {
        let trimmed = input.trim();
        let leading_ws = input.len() - input.trim_start().len();
        let current_offset = base_offset + leading_ws;

        if trimmed.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::EmptyPattern,
                span: current_offset..current_offset,
                message: "Empty pattern string".to_string(),
            });
        }

        if let Some((op_name, inner_str, op_span)) = parse_operator_syntax_or_error(trimmed, current_offset)? {
            if is_valid_op(&op_name) {
                let inner_pattern = Self::parse_inner(
                    &inner_str,
                    current_offset + trimmed.find('(').unwrap() + 1,
                    is_valid_op
                )?;
                return Ok(ParsedPattern::Operator {
                    name: op_name,
                    inner: Box::new(inner_pattern),
                });
            } else {
                return Err(ParseError {
                    kind: ParseErrorKind::InvalidOperator(op_name.clone()),
                    span: op_span,
                    message: format!("Invalid or unrecognized operator name '{}'", op_name),
                });
            }
        }

        let tokens = compile_pattern(trimmed).map_err(|mut e| {
            e.span = (current_offset + e.span.start)..(current_offset + e.span.end);
            e
        })?;

        Ok(ParsedPattern::Leaf {
            pattern: trimmed.to_string(),
            tokens,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;
    use alloc::string::ToString;

    fn leaf(pat: &str) -> ParsedPattern {
        ParsedPattern::Leaf {
            pattern: pat.to_string(),
            tokens: compile_pattern(pat).unwrap(),
        }
    }

    #[test]
    fn test_parser_basic() {
        let pat1 = ParsedPattern::parse(".text*", |_| false).unwrap();
        assert_eq!(pat1, leaf(".text*"));

        let pat2 = ParsedPattern::parse("REVERSE(.text*)", |op| op == "REVERSE").unwrap();
        assert_eq!(
            pat2,
            ParsedPattern::Operator {
                name: "REVERSE".to_string(),
                inner: Box::new(leaf(".text*"))
            }
        );
    }

    #[test]
    fn test_parser_nested() {
        let pat = ParsedPattern::parse("REVERSE(SORT(.text*))", |op| op == "REVERSE" || op == "SORT").unwrap();
        assert_eq!(
            pat,
            ParsedPattern::Operator {
                name: "REVERSE".to_string(),
                inner: Box::new(ParsedPattern::Operator {
                    name: "SORT".to_string(),
                    inner: Box::new(leaf(".text*"))
                })
            }
        );
    }

    #[test]
    fn test_parser_errors() {
        // Unknown operator
        let err1 = ParsedPattern::parse("SORT_XYZ(.text*)", |op| op == "SORT").unwrap_err();
        assert!(matches!(err1.kind, ParseErrorKind::InvalidOperator(_)));
        assert_eq!(err1.span, 0..8);

        // Mismatched paren
        let err2 = ParsedPattern::parse("SORT(.text*", |op| op == "SORT").unwrap_err();
        assert!(matches!(err2.kind, ParseErrorKind::MismatchedParentheses));
        assert_eq!(err2.span, 4..5);

        // Unexpected trailing chars
        let err3 = ParsedPattern::parse("SORT(.text*)foo", |op| op == "SORT").unwrap_err();
        assert!(matches!(err3.kind, ParseErrorKind::UnexpectedTrailingCharacters));
        assert_eq!(err3.span, 12..15);
    }
}
