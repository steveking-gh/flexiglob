#![cfg_attr(not(feature = "fs"), no_std)]

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use alloc::format;
use core::ops::Range;

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

/// Characters that may follow a backslash escape in a flexiglob pattern.
/// Everything else is either a path separator (use `/`) or needs no escaping.
const GLOB_ESCAPABLE: &[char] = &['*', '?', '[', ']', '(', ')', '"', '\\'];

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

/// Matches `candidate` against compiled `tokens` using Nondeterministic Finite
/// Automaton (NFA) simulation.
///
/// Maps each token to one NFA state; state `n` (= `tokens.len()`) is the sole
/// accepting state:
///
/// ```text
/// tokens:  [ T0  |  T1  |  T2  | ... | T(n-1) ]
/// states:    0      1      2    ...    n-1       n = accept
/// ```
///
/// A `Vec<bool>` (`active`) records all states simultaneously reachable given
/// the candidate prefix consumed so far.  Each new character advances the
/// simulation through two phases:
///
/// Phase 1 -- Consuming step.  For each active state i, fires the transition
/// matching the current character c:
///
/// ```text
/// Token              Condition        Adds to next
/// -----------------  ---------------  ----------------
/// Char(ch)           c == ch          i+1
/// AnyChar            c not in {/,\}   i+1
/// Set(s)             c in s           i+1
/// AnySeqNoSeparator  c not in {/,\}   i  (self-loop)
/// AnySeq             any c            i  (self-loop)
/// ```
///
/// Wildcard self-loops keep state i active so the wildcard can consume further
/// characters.  Advancement to i+1 (the zero-match case) follows in Phase 2.
///
/// Phase 2 -- Epsilon closure.  Propagates through zero-width transitions
/// without consuming any character.  See [`epsilon_close`].
///
/// Complexity: O(n x m) time, O(n) space -- n = token count, m = candidate
/// length.  The active-state set stays bounded by n+1, eliminating the
/// exponential backtracking of naive recursive implementations.
pub fn wildcard_match(tokens: &[MatchToken], candidate: &str) -> bool {
    let n = tokens.len();

    // Two fixed-size bit-vectors; allocate once and swap each iteration.
    let mut active = alloc::vec![false; n + 1];
    let mut next   = alloc::vec![false; n + 1];

    active[0] = true;
    epsilon_close(&mut active, tokens); // settle any leading zero-width tokens

    for c in candidate.chars() {
        // --- Phase 1: consuming step ---
        next.fill(false);
        for i in 0..n {
            if !active[i] { continue; }
            match &tokens[i] {
                MatchToken::Char(ch) if c == *ch => {
                    next[i + 1] = true;
                }
                MatchToken::AnyChar if c != '/' => {
                    next[i + 1] = true;
                }
                MatchToken::Set(set) if set.contains(&c) => {
                    next[i + 1] = true;
                }
                MatchToken::NegatedSet(set) if c != '/' && !set.contains(&c) => {
                    next[i + 1] = true;
                }
                // Self-loop: wildcard consumes c and stays at i, allowing
                // further characters to match.  epsilon_close below adds
                // i+1, covering the case where the wildcard matches nothing
                // further after this character.
                MatchToken::AnySeqNoSeparator if c != '/' => {
                    next[i] = true;
                }
                MatchToken::AnySeq => {
                    next[i] = true; // ** self-loops on any character, including separators
                }
                _ => {}
            }
        }

        // --- Phase 2: epsilon closure ---
        epsilon_close(&mut next, tokens);
        core::mem::swap(&mut active, &mut next);
    }

    active[n] // accept iff the end-of-pattern state is reachable
}

fn is_separator_token(tok: &MatchToken) -> bool {
    matches!(tok, MatchToken::Char('/'))
}

/// Propagates epsilon (zero-width) transitions forward through `active`.
///
/// Only AnySeqNoSeparator and AnySeq carry epsilon transitions; every other
/// token requires exactly one character to advance.
///
/// ```text
/// AnySeqNoSeparator (*) at state i:
///   i --eps--> i+1        (* matches zero characters)
///
/// AnySeq (**) at state i:
///   i --eps--> i+1        (** matches zero characters)
///   i --eps--> i+2        (only when tokens[i+1] is '/' or '\')
/// ```
///
/// The second AnySeq epsilon -- the "separator skip" -- collapses ** and an
/// adjacent pattern separator into zero path components.  This enables a
/// pattern like "src/**/*.rs" to match "src/lib.rs" (zero sub-directories):
///
/// ```text
/// Pattern fragment:  **   /   *      (states i, i+1, i+2)
///
///   i --eps--> i+1                   (standard: ** = zero chars)
///   i --eps--> i+2                   (separator skip)
///              ^
///              ** absorbs tokens[i+1] = Char('/'),
///              so the **/ pair matches zero path components
/// ```
///
/// A single left-to-right pass handles all transitivity because every
/// epsilon edge points strictly forward (i -> i+1 or i -> i+2).  Forward-only
/// epsilon edges prevent any state from receiving a second visit, guaranteeing
/// O(n) termination per call.
fn epsilon_close(active: &mut [bool], tokens: &[MatchToken]) {
    let n = tokens.len();
    for i in 0..n {
        if !active[i] { continue; }
        match &tokens[i] {
            MatchToken::AnySeqNoSeparator => {
                active[i + 1] = true;
            }
            MatchToken::AnySeq => {
                active[i + 1] = true;
                // Separator skip: when the very next token is a literal path
                // separator, also mark i+2 so that the ** + separator pair
                // collapses to zero path components as a unit.
                // Bounds: i+1 < n guarantees tokens[i+1] exists;
                //         i+2 <= n guarantees active[i+2] is in-bounds.
                if i + 1 < n && is_separator_token(&tokens[i + 1]) {
                    active[i + 2] = true;
                }
            }
            _ => {}
        }
    }
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

/// The operator trait for customizing query step transformations.
pub trait GlobOperator<T> {
    /// The operator name used for pattern matching.
    fn name(&self) -> &str;

    /// Reorders or filters the matched candidate references in-place.
    fn apply(&self, candidates: &mut Vec<&T>);
}

/// A ready-to-use operator that reverses the matched candidate list.
pub struct ReverseOp;

impl<T> GlobOperator<T> for ReverseOp {
    fn name(&self) -> &str { "REVERSE" }
    fn apply(&self, candidates: &mut Vec<&T>) {
        candidates.reverse();
    }
}

/// A generic closure-based operator wrapper.
// for<'a>: apply() receives references tied to the candidates slice —
// lifetime unknown at FnOperator construction — F must accept any 'a.
pub struct FnOperator<T, F>
where
    F: for<'a> Fn(&mut Vec<&'a T>),
{
    name: &'static str,
    func: F,
    _marker: core::marker::PhantomData<T>,
}

impl<T, F> FnOperator<T, F>
where
    F: for<'a> Fn(&mut Vec<&'a T>),
{
    /// Creates a new closure-based operator wrapper with the specified static name.
    pub fn new(name: &'static str, func: F) -> Self {
        Self {
            name,
            func,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<T, F> GlobOperator<T> for FnOperator<T, F>
where
    F: for<'a> Fn(&mut Vec<&'a T>),
{
    fn name(&self) -> &str {
        self.name
    }

    fn apply(&self, candidates: &mut Vec<&T>) {
        (self.func)(candidates);
    }
}

/// The matching and operator execution engine builder registry.
pub struct GlobberBuilder<'a, T> {
    operators: BTreeMap<String, Box<dyn GlobOperator<T> + 'a>>,
}

impl<'a, T> core::fmt::Debug for GlobberBuilder<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GlobberBuilder")
            .field("operators", &self.operators.keys())
            .finish()
    }
}

impl<'a, T> Default for GlobberBuilder<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> GlobberBuilder<'a, T> {
    /// Constructs a new, empty builder registry context.
    pub fn new() -> Self {
        Self {
            operators: BTreeMap::new(),
        }
    }

    /// Builder-style custom operator registration.
    pub fn with_operator(mut self, op: impl GlobOperator<T> + 'a) -> Self {
        self.register_operator(op);
        self
    }

    /// Registers a custom pipeline operator.
    pub fn register_operator(&mut self, op: impl GlobOperator<T> + 'a) {
        self.operators.insert(op.name().to_string(), Box::new(op));
    }

    /// Compiles a pattern string and validates that any nested operator name is registered.
    pub fn compile(&'a self, pattern: &str) -> Result<Globber<'a, T>, ParseError> {
        let parsed = ParsedPattern::parse(pattern, |op| self.operators.contains_key(op))?;
        Ok(Globber {
            pattern: parsed,
            operators: &self.operators,
        })
    }

}

/// Filesystem traversal hint produced by [`Globber::scan_hint`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanHint<'a> {
    /// Static path prefix before the first wildcard — the minimum root to scan.
    pub root: &'a str,
    /// True when the pattern contains `**`, requiring recursive traversal.
    pub is_recursive: bool,
    /// True when the pattern contains no wildcards — `root` is an exact path.
    /// Skip directory traversal and probe with `Path::exists()` directly.
    pub is_literal: bool,
}

/// The compiled matching and operator execution engine.
pub struct Globber<'a, T> {
    pattern: ParsedPattern,
    operators: &'a BTreeMap<String, Box<dyn GlobOperator<T> + 'a>>,
}

impl<'a, T> core::fmt::Debug for Globber<'a, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Globber")
            .field("pattern", &self.pattern)
            .field("operators", &self.operators.keys())
            .finish()
    }
}

impl<'a, T> Globber<'a, T> {
    /// Evaluates the compiled pattern against a candidate list, returning
    /// references into the original slice. No `Clone` bound is required.
    pub fn run<'c>(
        &self,
        candidates: &'c [T],
        get_name: impl Fn(&T) -> &str,
    ) -> Vec<&'c T>
    {
        self.run_inner(&self.pattern, candidates, &get_name)
    }

    fn run_inner<'c>(
        &self,
        pattern: &ParsedPattern,
        candidates: &'c [T],
        get_name: &impl Fn(&T) -> &str,
    ) -> Vec<&'c T>
    {
        match pattern {
            ParsedPattern::Leaf { tokens, .. } => {
                candidates
                    .iter()
                    .filter(|c| wildcard_match(tokens, get_name(c)))
                    .collect()
            }
            ParsedPattern::Operator { name, inner } => {
                let mut matched = self.run_inner(inner, candidates, get_name);
                if let Some(op) = self.operators.get(name) {
                    op.apply(&mut matched);
                } else {
                    unreachable!("operator '{}' in AST but not in registry; compile() prevents this", name);
                }
                matched
            }
        }
    }

    /// Returns a `ScanHint` describing the minimum filesystem traversal needed
    /// to build a complete candidate set for this pattern.
    ///
    /// ```text
    /// Pattern                  root                  recursive  is_literal
    /// -------                  ----                  ---------  ----------
    /// src/**/*.rs              "src/"                true       false
    /// src/parser/*.rs          "src/parser/"         false      false
    /// src/parser/ast.rs        "src/parser/ast.rs"   false      true
    /// .text*                   ""                    false      false
    /// src/foo\*.rs             "src/foo\*.rs"        false      true
    /// SORT_SIZE(src/**/*.rs)   "src/"                true       false
    /// ```
    ///
    /// Operator wrappers are transparent — traversal descends to the leaf.
    /// Escaped wildcard characters (e.g. `\*`) do not count as wildcards.
    pub fn scan_hint(&self) -> ScanHint<'_> {
        let (pattern, tokens) = Self::find_leaf(&self.pattern);

        // Locate the byte position of the first unescaped wildcard character.
        let mut iter = pattern.char_indices();
        let mut wildcard_pos: Option<usize> = None;
        while let Some((pos, ch)) = iter.next() {
            if ch == '\\' {
                iter.next(); // skip the escaped character
            } else if matches!(ch, '*' | '?' | '[') {
                wildcard_pos = Some(pos);
                break;
            }
        }

        let root = match wildcard_pos {
            None => pattern, // no wildcards: full pattern is the path
            Some(pos) => match pattern[..pos].rfind('/') {
                None => "",
                Some(sep) => &pattern[..sep + 1],
            },
        };

        let recursive = tokens.iter().any(|t| matches!(t, MatchToken::AnySeq));
        let is_literal = tokens.iter().all(|t| matches!(t, MatchToken::Char(_)));

        ScanHint { root, is_recursive: recursive, is_literal }
    }

    fn find_leaf(pattern: &ParsedPattern) -> (&str, &[MatchToken]) {
        match pattern {
            ParsedPattern::Leaf { pattern, tokens } => (pattern.as_str(), tokens),
            ParsedPattern::Operator { inner, .. } => Self::find_leaf(inner),
        }
    }
}

#[cfg(feature = "fs")]
impl<'a> Globber<'a, String> {
    /// Evaluates the compiled pattern against the local filesystem.
    ///
    /// Uses `scan_hint()` to determine the root to scan, enumerates candidates
    /// from the filesystem, then runs the full match-and-operator pipeline.
    /// All returned paths use forward slashes regardless of platform.
    pub fn run_fs(&self) -> Vec<String> {
        let hint = self.scan_hint();
        let candidates = fs_impl::enumerate_candidates(&hint);
        self.run(&candidates, |s| s.as_str())
            .into_iter()
            .cloned()
            .collect()
    }
}

#[cfg(feature = "fs")]
mod fs_impl {
    use crate::ScanHint;
    use std::{
        collections::HashSet,
        fs,
        path::{Path, PathBuf},
    };

    pub(crate) fn enumerate_candidates(hint: &ScanHint<'_>) -> Vec<String> {
        let root = if hint.root.is_empty() { "." } else { hint.root };
        let mut candidates = Vec::new();

        if hint.is_literal {
            // Skip traversal: return the path as-is so the caller's I/O layer
            // produces a precise OS error if the file is absent, rather than
            // silently returning no matches.
            candidates.push(normalize(root));
        } else {
            let root_path = Path::new(root);
            if root_path.is_dir() {
                let mut visited = HashSet::new();
                if let Ok(canonical) = root_path.canonicalize() {
                    visited.insert(canonical);
                }
                collect(root_path, hint.is_recursive, &mut visited, &mut candidates);
                candidates.sort();
            }
        }
        candidates
    }

    fn normalize(p: &str) -> String {
        p.replace('\\', "/")
    }

    fn collect(dir: &Path, recursive: bool, visited: &mut HashSet<PathBuf>, out: &mut Vec<String>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            // metadata() follows symlinks: symlinks to files/dirs are treated as
            // their target type; broken symlinks are silently skipped.
            let Ok(meta) = fs::metadata(&path) else { continue };

            if meta.is_file() {
                if let Some(s) = path.to_str() {
                    out.push(normalize(s));
                }
            } else if meta.is_dir() && recursive {
                // canonicalize() resolves the real path behind any symlinks.
                // If this directory was already visited we have a cycle; skip it.
                if let Ok(canonical) = path.canonicalize() {
                    if visited.insert(canonical) {
                        collect(&path, recursive, visited, out);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn test_wildcard_matching() {
        let tok1 = compile_pattern("*.elf").unwrap();
        assert!(wildcard_match(&tok1, "main.elf"));
        assert!(wildcard_match(&tok1, "utils.elf"));
        assert!(!wildcard_match(&tok1, "main.elf.old"));

        let tok2 = compile_pattern("a?b").unwrap();
        assert!(wildcard_match(&tok2, "axb"));
        assert!(wildcard_match(&tok2, "a_b"));
        assert!(!wildcard_match(&tok2, "ab"));
        assert!(!wildcard_match(&tok2, "axxxb"));

        let tok3 = compile_pattern("*.foo[12]").unwrap();
        assert!(wildcard_match(&tok3, "test.foo1"));
        assert!(wildcard_match(&tok3, "test.foo2"));
        assert!(!wildcard_match(&tok3, "test.foo3"));

        let tok4 = compile_pattern("foo\\*bar").unwrap();
        assert!(wildcard_match(&tok4, "foo*bar"));
        assert!(!wildcard_match(&tok4, "fooxbar"));

        // Test all escape characters specified in README:
        // \* (star), \? (question), \[ (open bracket), \] (close bracket), \" (double quote), \\ (backslash)
        let tok5 = compile_pattern("a\\*b\\?c\\[d\\]e\\\"f\\\\g").unwrap();
        assert!(wildcard_match(&tok5, "a*b?c[d]e\"f\\g"));
        assert!(!wildcard_match(&tok5, "axbxc[d]e\"f\\g"));
    }

    #[test]
    fn test_backtracking() {
        let tok = compile_pattern("a*b*c").unwrap();
        assert!(wildcard_match(&tok, "abc"));
        assert!(wildcard_match(&tok, "axbxc"));
        assert!(wildcard_match(&tok, "abxbxc"));
        assert!(!wildcard_match(&tok, "axb"));
    }

    #[test]
    fn test_ranges() {
        let tok = compile_pattern("foo[a-z0-9]").unwrap();
        assert!(wildcard_match(&tok, "fooa"));
        assert!(wildcard_match(&tok, "foo5"));
        assert!(!wildcard_match(&tok, "fooA"));
    }

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

    #[derive(Clone, Debug, PartialEq)]
    struct Item {
        name: String,
        val: u32,
    }

    struct SortValOp;
    impl GlobOperator<Item> for SortValOp {
        fn name(&self) -> &str { "SORT_VAL" }
        fn apply(&self, candidates: &mut Vec<&Item>) {
            candidates.sort_by_key(|c| c.val);
        }
    }

    #[test]
    fn test_pipeline_execution() {
        let builder = GlobberBuilder::new()
            .with_operator(ReverseOp)
            .with_operator(SortValOp);
        let globber = builder.compile("REVERSE(SORT_VAL(item*))").unwrap();

        let candidates = vec![
            Item { name: "item1".to_string(), val: 30 },
            Item { name: "item2".to_string(), val: 10 },
            Item { name: "item3".to_string(), val: 20 },
        ];

        // Standard evaluation
        let result = globber.run(&candidates, |item| &item.name);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "item1"); // Sorted would be item2(10), item3(20), item1(30); Reversed is 1, 3, 2.
        assert_eq!(result[1].name, "item3");
        assert_eq!(result[2].name, "item2");

    }

    #[test]
    fn test_fn_operator() {
        let builder = GlobberBuilder::new()
            .with_operator(ReverseOp)
            .with_operator(FnOperator::new("SORT_VAL", |candidates: &mut Vec<&Item>| {
                candidates.sort_by_key(|c| c.val);
            }));
        let globber = builder.compile("REVERSE(SORT_VAL(item*))").unwrap();

        let candidates = vec![
            Item { name: "item1".to_string(), val: 30 },
            Item { name: "item2".to_string(), val: 10 },
            Item { name: "item3".to_string(), val: 20 },
        ];

        let result = globber.run(&candidates, |item| &item.name);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "item1");
        assert_eq!(result[1].name, "item3");
        assert_eq!(result[2].name, "item2");
    }

    #[test]
    fn test_path_aware_matching() {
        // Test single wildcard '*' does not match separators
        let tok_star = compile_pattern("src/*.rs").unwrap();
        assert!(wildcard_match(&tok_star, "src/lib.rs"));
        assert!(wildcard_match(&tok_star, "src/main.rs"));
        assert!(!wildcard_match(&tok_star, "src/parser/ast.rs"));
        assert!(!wildcard_match(&tok_star, "lib.rs"));

        // Test double wildcard '**' matches recursively (including separators)
        let tok_globstar = compile_pattern("src/**/*.rs").unwrap();
        assert!(wildcard_match(&tok_globstar, "src/lib.rs"));
        assert!(wildcard_match(&tok_globstar, "src/parser/ast.rs"));
        assert!(wildcard_match(&tok_globstar, "src/parser/ast/node.rs"));
        assert!(!wildcard_match(&tok_globstar, "lib.rs"));

        // Test question mark '?' does not match separators
        let tok_question = compile_pattern("a?b").unwrap();
        assert!(wildcard_match(&tok_question, "axb"));
        assert!(!wildcard_match(&tok_question, "a/b"));
        assert!(wildcard_match(&tok_question, "a\\b")); // '/' is the only separator; '\' is an ordinary char in candidates

        // Test complex nested matching with stack backtracking
        let tok_complex = compile_pattern("**/a/*").unwrap();
        assert!(wildcard_match(&tok_complex, "x/a/y"));
        assert!(wildcard_match(&tok_complex, "x/a/y/a/z"));
        assert!(!wildcard_match(&tok_complex, "x/a/y/z")); // second wildcard is single '*', cannot match 'y/z'
    }
}
