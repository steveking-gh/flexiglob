# Flexiglob Implementation Plan

This document outlines the step-by-step implementation plan for the `flexiglob` crate, a generic, freestanding wildcard query pipeline engine for Rust.

---

## Architecture & AST Design

We will define the core representations in [flexiglob/src/lib.rs](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs):

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedPattern {
    /// A leaf string containing wildcards (e.g. ".text*").
    Leaf(String),
    
    /// A pipeline operator (e.g. "REVERSE", "SORT", "SORT_BY_ALIGNMENT").
    Operator {
        name: String,
        inner: Box<ParsedPattern>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchToken {
    /// A literal character.
    Char(char),
    
    /// Matches exactly one character (represented by '?').
    AnyChar,
    
    /// Matches zero or more characters (represented by '*').
    AnySeq,
    
    /// Matches any character in the set (represented by '[chars]').
    Set(std::collections::HashSet<char>),
}
```

---

## Error Diagnostics Design

To allow callers to generate beautiful compiler diagnostics using crates like `ariadne`, the parser will return relative character byte ranges mapping the exact locations of syntax errors.

```rust
use std::ops::Range;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    EmptyPattern,
    InvalidOperator(String),
    MismatchedParentheses,
    UnterminatedBracketSet,
    UnexpectedTrailingCharacters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The type of syntax error that occurred.
    pub kind: ParseErrorKind,
    
    /// The span of the offending characters relative to the input string start.
    pub span: Range<usize>,
    
    /// A default human-readable explanation of the error.
    pub message: String,
}
```

---

## Pipeline Tracing & Debugging Design (Option 1)

For debugging intermediate pipeline evaluation steps without imposing third-party logging library dependencies, we implement Option 1 (Callback Hook) via a lightweight closure-based trace callback:

```rust
pub type TraceCallback<'a, T> = dyn Fn(&str, &[T]) + 'a;
```

To make tracing completely optional and require zero boilerplate for standard use cases:
1. The standard `match_and_eval` method does not require a callback and internally uses a default no-op trace implementation.
2. The `match_and_eval_with_trace` method accepts a `&TraceCallback<'_, T>` to execute custom hooks.
3. The library will also expose a standard no-op constant/closure helper `flexiglob::NOOP_TRACE` (defined as `|_: &str, _: &[T]| {}`) for generic use.

---

## Step-by-Step Implementation Steps

### Step 1: Wildcard Matcher
Implement compilation of wildcard strings into tokens and the backtrack-based matching algorithm.

* **API**:
  - `fn compile_pattern(pattern: &str) -> Result<Vec<MatchToken>, ParseError>`
  - `fn wildcard_match(tokens: &[MatchToken], candidate: &str) -> bool`
* **Rules Handled**:
  - `?` matches exactly one character.
  - `*` matches 0 or more characters (with backtracking).
  - `[<chars>]` matches any character in the bracket set (raising `UnterminatedBracketSet` if unclosed).
  - Backslash escapes: `\*`, `\?`, `\[`, `\]`, `\"`, `\\` treat the second character as a literal match.
* **Verification (Unit Tests)**:
  - Literal equality.
  - Single and multiple `*` wildcards (e.g., `*a*b*c` against `xaybzc`).
  - Brackets set matching (e.g., `[abc]` against `b`, and escaping brackets).
  - Escaped wildcards (e.g., `\*` against `*`).

---

### Step 2: DSL Parser
Implement the recursive descent parser to parse nested operators and produce precise span-based errors.

* **API**:
  - `ParsedPattern::parse(input: &str, is_valid_op: impl Fn(&str) -> bool) -> Result<Self, ParseError>`
* **Logic**:
  - The parser trims the input string.
  - It identifies if it starts with an uppercase identifier and `(` and ends with `)`.
  - It finds the matching closing parenthesis for the outer operator and parses the inner content recursively.
  - Checks if the parsed operator name is valid using the validator closure (returning `ParseErrorKind::InvalidOperator` on failure, with the span covering the operator name).
  - If no operator wrapper is matched, treats the whole string as a `ParsedPattern::Leaf`.
* **Verification (Unit Tests)**:
  - Leaf pattern parsing (e.g., `.text*`).
  - Nested operators (e.g., `REVERSE(SORT(.text*))`).
  - Validation failures (e.g., parsing `UNKNOWN_OP(.text*)` fails and returns span `0..10`).
  - Malformed parentheses errors (e.g., `SORT(.text*` returns span pointing to the opening parenthesis).

---

### Step 3: Pipeline Evaluation Engine
Implement the traits, matching context registry, and debug hooks.

* **API**:
  ```rust
  pub trait GlobOperator<T> {
      fn name(&self) -> &str;
      fn apply(&self, candidates: &mut Vec<T>);
  }

  pub struct Globber<'a, T> {
      operators: std::collections::HashMap<String, Box<dyn GlobOperator<T> + 'a>>,
  }
  
  impl<'a, T> Globber<'a, T> {
      pub fn match_and_eval(
          &self,
          pattern: &ParsedPattern,
          candidates: &[T],
          get_name: impl Fn(&T) -> &str,
      ) -> Vec<T>;

      pub fn match_and_eval_with_trace(
          &self,
          pattern: &ParsedPattern,
          candidates: &[T],
          get_name: impl Fn(&T) -> &str,
          trace: &TraceCallback<'_, T>,
      ) -> Vec<T>;
  }
  ```
* **Logic**:
  - `Globber::register_operator` registers any custom or standard operator.
  - `Globber::match_and_eval` executes evaluation recursively:
    - Base case `Leaf`: Wildcard matches candidate names using `get_name` closure and returns matches.
    - Recursive case `Operator`: Evaluates `inner`, looks up operator in the registry, and calls `apply`.
* **Verification (Unit Tests)**:
  - Implement a test `struct TestItem { name: String, score: u32 }`.
  - Register a built-in `REVERSE` operator.
  - Register a custom `SORT_SCORE` operator.
  - Evaluate `REVERSE(SORT_SCORE(*))` against a list of `TestItem` and verify correct filtering, order, and trace callback messages.
