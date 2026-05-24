#![no_std]

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
}

/// The syntax error kinds returned during parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    EmptyPattern,
    InvalidOperator(String),
    MismatchedParentheses,
    UnterminatedBracketSet,
    UnexpectedTrailingCharacters,
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
                    tokens.push(MatchToken::Char(chars[i + 1].1));
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
                let mut set = BTreeSet::new();
                let mut closed = false;

                while i < chars.len() {
                    let cur_ch = chars[i].1;
                    if cur_ch == ']' {
                        closed = true;
                        i += 1;
                        break;
                    }
                    if cur_ch == '\\' {
                        if i + 1 < chars.len() {
                            set.insert(chars[i + 1].1);
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
                            // Invalid range, insert characters literally
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
                tokens.push(MatchToken::Set(set));
            }
            c => {
                tokens.push(MatchToken::Char(c));
                i += 1;
            }
        }
    }

    Ok(tokens)
}

/// Path-aware wildcard string matching algorithm.
pub fn wildcard_match(tokens: &[MatchToken], candidate: &str) -> bool {
    let candidate_chars: Vec<char> = candidate.chars().collect();
    wildcard_match_recursive(tokens, &candidate_chars, 0, 0)
}

fn is_separator_token(tok: &MatchToken) -> bool {
    matches!(tok, MatchToken::Char('/') | MatchToken::Char('\\'))
}

fn wildcard_match_recursive(
    tokens: &[MatchToken],
    candidate: &[char],
    t_idx: usize,
    c_idx: usize,
) -> bool {
    if t_idx == tokens.len() {
        return c_idx == candidate.len();
    }

    match &tokens[t_idx] {
        MatchToken::Char(c) => {
            if c_idx < candidate.len() && candidate[c_idx] == *c {
                wildcard_match_recursive(tokens, candidate, t_idx + 1, c_idx + 1)
            } else {
                false
            }
        }
        MatchToken::AnyChar => {
            if c_idx < candidate.len() && candidate[c_idx] != '/' && candidate[c_idx] != '\\' {
                wildcard_match_recursive(tokens, candidate, t_idx + 1, c_idx + 1)
            } else {
                false
            }
        }
        MatchToken::Set(set) => {
            if c_idx < candidate.len() && set.contains(&candidate[c_idx]) {
                wildcard_match_recursive(tokens, candidate, t_idx + 1, c_idx + 1)
            } else {
                false
            }
        }
        MatchToken::AnySeq => {
            // Option 1: ** matches zero directory components (collapsing a trailing slash in pattern)
            if t_idx + 1 < tokens.len()
                && is_separator_token(&tokens[t_idx + 1])
                && wildcard_match_recursive(tokens, candidate, t_idx + 2, c_idx)
            {
                return true;
            }

            // Option 2: ** matches zero or more characters (including separators)
            for len in 0..=(candidate.len() - c_idx) {
                if wildcard_match_recursive(tokens, candidate, t_idx + 1, c_idx + len) {
                    return true;
                }
            }
            false
        }
        MatchToken::AnySeqNoSeparator => {
            // * matches zero or more characters except separators.
            for len in 0..=(candidate.len() - c_idx) {
                if len > 0 && (candidate[c_idx + len - 1] == '/' || candidate[c_idx + len - 1] == '\\') {
                    break;
                }
                if wildcard_match_recursive(tokens, candidate, t_idx + 1, c_idx + len) {
                    return true;
                }
            }
            false
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

    // Validate op_name is a valid identifier
    if op_name.is_empty() {
        return Ok(None);
    }
    for (idx, c) in op_name.chars().enumerate() {
        if idx == 0 {
            if !c.is_alphabetic() && c != '_' {
                return Ok(None);
            }
        } else {
            if !c.is_alphanumeric() && c != '_' {
                return Ok(None);
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
            let op_name_upper = op_name.to_uppercase();
            if op_name_upper == "REVERSE" {
                let inner_pattern = Self::parse_inner(
                    &inner_str,
                    current_offset + trimmed.find('(').unwrap() + 1,
                    is_valid_op
                )?;
                return Ok(ParsedPattern::Operator {
                    name: "REVERSE".to_string(),
                    inner: Box::new(inner_pattern),
                });
            } else if is_valid_op(&op_name_upper) {
                let inner_pattern = Self::parse_inner(
                    &inner_str,
                    current_offset + trimmed.find('(').unwrap() + 1,
                    is_valid_op
                )?;
                return Ok(ParsedPattern::Operator {
                    name: op_name_upper,
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
    /// The uppercase operator name.
    fn name(&self) -> &str;

    /// Modifies matching candidate elements in-place.
    fn apply(&self, candidates: &mut Vec<T>);
}

/// A generic closure-based operator wrapper.
pub struct FnOperator<T, F>
where
    F: Fn(&mut Vec<T>),
{
    name: &'static str,
    func: F,
    _marker: core::marker::PhantomData<T>,
}

impl<T, F> FnOperator<T, F>
where
    F: Fn(&mut Vec<T>),
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
    F: Fn(&mut Vec<T>),
{
    fn name(&self) -> &str {
        self.name
    }

    fn apply(&self, candidates: &mut Vec<T>) {
        (self.func)(candidates);
    }
}

/// Closure-based trace logging callback type.
pub type TraceCallback<'a, T> = dyn Fn(&str, &[T]) + 'a;

/// The standard no-op trace callback helper function.
pub fn noop_trace<T>(_: &str, _: &[T]) {}

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
        self.operators.insert(op.name().to_uppercase(), Box::new(op));
    }

    /// Compiles a pattern string and validates that any nested operator name is registered.
    pub fn compile(self, pattern: &str) -> Result<Globber<'a, T>, ParseError> {
        let parsed = ParsedPattern::parse(pattern, |op| self.operators.contains_key(op))?;
        Ok(Globber {
            pattern: parsed,
            operators: self.operators,
        })
    }

}

/// The compiled matching and operator execution engine.
pub struct Globber<'a, T> {
    pattern: ParsedPattern,
    operators: BTreeMap<String, Box<dyn GlobOperator<T> + 'a>>,
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
    /// Evaluates the compiled pattern without generating any tracing output.
    pub fn run(
        &self,
        candidates: &[T],
        get_name: impl Fn(&T) -> &str,
    ) -> Result<Vec<T>, ParseError>
    where
        T: Clone,
    {
        self.run_inner(&self.pattern, candidates, &get_name, None)
    }

    /// Evaluates the compiled pattern while reporting progress to a custom trace hook.
    pub fn run_with_trace(
        &self,
        candidates: &[T],
        get_name: impl Fn(&T) -> &str,
        trace: &TraceCallback<'_, T>,
    ) -> Result<Vec<T>, ParseError>
    where
        T: Clone,
    {
        self.run_inner(&self.pattern, candidates, &get_name, Some(trace))
    }

    fn run_inner(
        &self,
        pattern: &ParsedPattern,
        candidates: &[T],
        get_name: &impl Fn(&T) -> &str,
        trace: Option<&TraceCallback<'_, T>>,
    ) -> Result<Vec<T>, ParseError>
    where
        T: Clone,
    {
        match pattern {
            ParsedPattern::Leaf { pattern, tokens } => {
                let matched: Vec<T> = candidates
                    .iter()
                    .filter(|c| wildcard_match(tokens, get_name(c)))
                    .cloned()
                    .collect();
                if let Some(t) = trace {
                    t(&format!("Leaf matched '{}': {} items", pattern, matched.len()), &matched);
                }
                Ok(matched)
            }
            ParsedPattern::Operator { name, inner } => {
                let mut matched = self.run_inner(inner, candidates, get_name, trace)?;
                if name == "REVERSE" {
                    matched.reverse();
                    if let Some(t) = trace {
                        t("Applied operator 'REVERSE'", &matched);
                    }
                } else if let Some(op) = self.operators.get(name) {
                    op.apply(&mut matched);
                    if let Some(t) = trace {
                        t(&format!("Applied operator '{}'", name), &matched);
                    }
                } else {
                    unreachable!("operator '{}' in AST but not in registry; compile() prevents this", name);
                }
                Ok(matched)
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

        let pat2 = ParsedPattern::parse("REVERSE(.text*)", |_| false).unwrap();
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
        let pat = ParsedPattern::parse("REVERSE(SORT(.text*))", |op| op == "SORT").unwrap();
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
        fn apply(&self, candidates: &mut Vec<Item>) {
            candidates.sort_by_key(|c| c.val);
        }
    }

    #[test]
    fn test_pipeline_execution() {
        let globber = GlobberBuilder::new()
            .with_operator(SortValOp)
            .compile("REVERSE(SORT_VAL(item*))")
            .unwrap();

        let candidates = vec![
            Item { name: "item1".to_string(), val: 30 },
            Item { name: "item2".to_string(), val: 10 },
            Item { name: "item3".to_string(), val: 20 },
        ];

        // Standard evaluation
        let result = globber.run(&candidates, |item| &item.name).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "item1"); // Sorted would be item2(10), item3(20), item1(30); Reversed is 1, 3, 2.
        assert_eq!(result[1].name, "item3");
        assert_eq!(result[2].name, "item2");

        // Traced evaluation
        let logs = core::cell::RefCell::new(Vec::new());
        let trace_cb = |msg: &str, items: &[Item]| {
            logs.borrow_mut().push(format!("{}: {:?}", msg, items.iter().map(|i| &i.name).collect::<Vec<_>>()));
        };
        let _ = globber.run_with_trace(&candidates, |item| &item.name, &trace_cb).unwrap();

        let logs_vec = logs.into_inner();
        assert_eq!(logs_vec.len(), 3);
        assert!(logs_vec[0].contains("Leaf matched"));
        assert!(logs_vec[1].contains("Applied operator 'SORT_VAL'"));
        assert!(logs_vec[2].contains("Applied operator 'REVERSE'"));
    }

    #[test]
    fn test_fn_operator() {
        let globber = GlobberBuilder::new()
            .with_operator(FnOperator::new("SORT_VAL", |candidates: &mut Vec<Item>| {
                candidates.sort_by_key(|c| c.val);
            }))
            .compile("REVERSE(SORT_VAL(item*))")
            .unwrap();

        let candidates = vec![
            Item { name: "item1".to_string(), val: 30 },
            Item { name: "item2".to_string(), val: 10 },
            Item { name: "item3".to_string(), val: 20 },
        ];

        let result = globber.run(&candidates, |item| &item.name).unwrap();
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
        assert!(!wildcard_match(&tok_question, "a\\b"));

        // Test complex nested matching with stack backtracking
        let tok_complex = compile_pattern("**/a/*").unwrap();
        assert!(wildcard_match(&tok_complex, "x/a/y"));
        assert!(wildcard_match(&tok_complex, "x/a/y/a/z"));
        assert!(!wildcard_match(&tok_complex, "x/a/y/z")); // second wildcard is single '*', cannot match 'y/z'
    }
}
