use flexiglob::{FnOperator, GlobOperator, GlobberBuilder, ParsedPattern, ParseErrorKind, ReverseOp};

fn leaf(pat: &str) -> ParsedPattern {
    ParsedPattern::Leaf {
        pattern: pat.to_string(),
        tokens: flexiglob::compile_pattern(pat).unwrap(),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TargetSection {
    file: String,
    name: String,
    align: u64,
    size: u64,
}

// A custom operator to sort sections alphabetically by name.
struct SortByNameOp;
impl GlobOperator<TargetSection> for SortByNameOp {
    fn name(&self) -> &str { "SORT" }
    fn apply(&self, candidates: &mut Vec<&TargetSection>) {
        candidates.sort_by(|a, b| a.name.cmp(&b.name));
    }
}

// A custom operator to sort sections by alignment size (descending).
struct SortByAlignmentOp;
impl GlobOperator<TargetSection> for SortByAlignmentOp {
    fn name(&self) -> &str { "SORT_BY_ALIGNMENT" }
    fn apply(&self, candidates: &mut Vec<&TargetSection>) {
        candidates.sort_by_key(|c| core::cmp::Reverse(c.align));
    }
}

// A custom operator to filter out sections smaller than a minimum size.
struct FilterMinSizeOp {
    min_size: u64,
}
impl GlobOperator<TargetSection> for FilterMinSizeOp {
    fn name(&self) -> &str { "FILTER_MIN_SIZE" }
    fn apply(&self, candidates: &mut Vec<&TargetSection>) {
        candidates.retain(|c| c.size >= self.min_size);
    }
}

#[test]
fn test_flexiglob_pipeline_integration() {
    let builder = GlobberBuilder::new()
        .with_operator(SortByNameOp)
        .with_operator(SortByAlignmentOp)
        .with_operator(FilterMinSizeOp { min_size: 8 });
    let globber = builder.compile("SORT_BY_ALIGNMENT(FILTER_MIN_SIZE(.text*))").unwrap();

    let candidates = vec![
        TargetSection { file: "a.elf".to_string(), name: ".text.boot".to_string(), align: 4, size: 8 },
        TargetSection { file: "a.elf".to_string(), name: ".text.main".to_string(), align: 8, size: 32 },
        TargetSection { file: "b.elf".to_string(), name: ".text.handler".to_string(), align: 16, size: 24 },
        TargetSection { file: "b.elf".to_string(), name: ".data.debug".to_string(), align: 4, size: 64 },
        TargetSection { file: "c.elf".to_string(), name: ".text.init".to_string(), align: 4, size: 4 }, // too small
    ];

    let result = globber.run(&candidates, |s| &s.name);

    // Verify filtering and sorting order
    assert_eq!(result.len(), 3);
    // .text.init (size 4) should be filtered out by FILTER_MIN_SIZE
    // .data.debug (name doesn't match wildcard .text*) should be filtered out by wildcard
    // Remaining: .text.boot (align 4), .text.main (align 8), .text.handler (align 16)
    // Sorted by alignment descending:
    assert_eq!(result[0].name, ".text.handler");
    assert_eq!(result[1].name, ".text.main");
    assert_eq!(result[2].name, ".text.boot");
}

#[test]
fn test_flexiglob_nested_reverse() {
    let builder = GlobberBuilder::new()
        .with_operator(ReverseOp)
        .with_operator(SortByNameOp);
    let globber = builder.compile("REVERSE(SORT(.text*))").unwrap();

    let candidates = vec![
        TargetSection { file: "a.elf".to_string(), name: ".text.a".to_string(), align: 4, size: 32 },
        TargetSection { file: "a.elf".to_string(), name: ".text.c".to_string(), align: 4, size: 32 },
        TargetSection { file: "b.elf".to_string(), name: ".text.b".to_string(), align: 4, size: 32 },
    ];

    let result = globber.run(&candidates, |s| &s.name);

    assert_eq!(result.len(), 3);
    // Sorted: .text.a, .text.b, .text.c
    // Reversed: .text.c, .text.b, .text.a
    assert_eq!(result[0].name, ".text.c");
    assert_eq!(result[1].name, ".text.b");
    assert_eq!(result[2].name, ".text.a");
}

#[test]
fn test_flexiglob_invalid_inputs() {
    let builder = GlobberBuilder::new()
        .with_operator(ReverseOp)
        .with_operator(SortByNameOp);

    // Invalid operator nested in REVERSE
    let err = builder.compile("REVERSE(UNKNOWN_OP(.text*))").unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::InvalidOperator(ref name) if name == "UNKNOWN_OP"));
    assert_eq!(err.span, 8..18); // UNKNOWN_OP starts at offset 8 relative to outer start

    // Invalid operator nested inside a custom valid operator (SORT)
    let builder2 = GlobberBuilder::new().with_operator(SortByNameOp);
    let err2 = builder2.compile("SORT(UNKNOWN_OP(.text*))").unwrap_err();
    assert!(matches!(err2.kind, ParseErrorKind::InvalidOperator(ref name) if name == "UNKNOWN_OP"));
    assert_eq!(err2.span, 5..15); // UNKNOWN_OP starts at offset 5 relative to outer start
}

#[test]
fn test_multi_glob_not_supported() {
    let builder = GlobberBuilder::new()
        .with_operator(ReverseOp)
        .with_operator(SortByNameOp);

    // "REVERSE(*.txt(SORT(**))" — 3 opens, 2 closes.
    // The parser commits to REVERSE as an operator, then cannot find a matching ')' for
    // its opening paren (depth never returns to zero), so it errors.
    let err = builder.compile("REVERSE(*.txt(SORT(**))").unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::MismatchedParentheses));
    assert_eq!(err.span, 7..8); // the '(' immediately after REVERSE

    // With balanced parens — "REVERSE(*.txt(SORT(**)))" — the '(' after '*.txt'
    // is not preceded by a valid operator name, so it now errors rather than
    // silently treating the whole inner string as a leaf glob.
    let err2 = builder.compile("REVERSE(*.txt(SORT(**)))").unwrap_err();
    assert!(matches!(err2.kind, ParseErrorKind::UnexpectedParen));
    assert_eq!(err2.span, 13..14); // the '(' immediately after '*.txt'
}

#[test]
fn test_display_error() {
    let err = ParsedPattern::parse("", |_| false).unwrap_err();
    assert_eq!(err.to_string(), "Empty pattern string at 0..0");
}

#[test]
fn test_compile_pattern_edge_cases() {
    // Dangling backslash at end of pattern
    let err1 = flexiglob::compile_pattern("foo\\").unwrap_err();
    assert!(matches!(err1.kind, ParseErrorKind::UnexpectedTrailingCharacters));

    // Dangling backslash inside bracket set
    let err2 = flexiglob::compile_pattern("[abc\\").unwrap_err();
    assert!(matches!(err2.kind, ParseErrorKind::UnterminatedBracketSet));

    // Unterminated bracket set
    let err3 = flexiglob::compile_pattern("[abc").unwrap_err();
    assert!(matches!(err3.kind, ParseErrorKind::UnterminatedBracketSet));

    // Backslash escaping inside bracket set
    let tok = flexiglob::compile_pattern("[a\\\\b]").unwrap().to_vec();
    assert!(flexiglob::wildcard_match(&tok, "\\"));
    assert!(flexiglob::wildcard_match(&tok, "a"));
    assert!(flexiglob::wildcard_match(&tok, "b"));
    assert!(!flexiglob::wildcard_match(&tok, "c"));
}

#[test]
fn test_scan_hint() {
    use flexiglob::ScanHint;

    let builder = GlobberBuilder::<String>::new();

    // ** pattern: recursive, prefix up to last separator
    let g = builder.compile("src/**/*.rs").unwrap();
    assert_eq!(g.scan_hint(), ScanHint { root: "src/", recursive: true });

    // Single * pattern: not recursive, prefix up to last separator
    let g2 = builder.compile("src/parser/*.rs").unwrap();
    assert_eq!(g2.scan_hint(), ScanHint { root: "src/parser/", recursive: false });

    // No path separator before wildcard
    let g3 = builder.compile(".text*").unwrap();
    assert_eq!(g3.scan_hint(), ScanHint { root: "", recursive: false });

    // No wildcards: full path, not recursive
    let g4 = builder.compile("src/parser/ast.rs").unwrap();
    assert_eq!(g4.scan_hint(), ScanHint { root: "src/parser/ast.rs", recursive: false });

    // Escaped wildcard not counted — real wildcard is later
    let g5 = builder.compile("src/foo\\*.bar/baz*").unwrap();
    assert_eq!(g5.scan_hint(), ScanHint { root: "src/foo\\*.bar/", recursive: false });

    // Operator wrapper is transparent
    let builder2 = GlobberBuilder::<String>::new().with_operator(flexiglob::ReverseOp);
    let g6 = builder2.compile("REVERSE(src/**/*.rs)").unwrap();
    assert_eq!(g6.scan_hint(), ScanHint { root: "src/", recursive: true });
}

#[test]
fn test_negated_bracket_sets() {
    // Basic negation: matches chars not in set
    let tok = flexiglob::compile_pattern("[^abc]").unwrap();
    assert!(flexiglob::wildcard_match(&tok, "d"));
    assert!(flexiglob::wildcard_match(&tok, "z"));
    assert!(!flexiglob::wildcard_match(&tok, "a"));
    assert!(!flexiglob::wildcard_match(&tok, "b"));
    assert!(!flexiglob::wildcard_match(&tok, "c"));

    // Negated set does not match path separators
    assert!(!flexiglob::wildcard_match(&tok, "/"));
    assert!(!flexiglob::wildcard_match(&tok, "\\"));

    // Negated range
    let tok2 = flexiglob::compile_pattern("[^a-z]").unwrap();
    assert!(flexiglob::wildcard_match(&tok2, "A"));
    assert!(flexiglob::wildcard_match(&tok2, "0"));
    assert!(!flexiglob::wildcard_match(&tok2, "m"));

    // Negated set in a full pattern
    let tok3 = flexiglob::compile_pattern(".text[^0-9]").unwrap();
    assert!(flexiglob::wildcard_match(&tok3, ".texta"));
    assert!(!flexiglob::wildcard_match(&tok3, ".text5"));
    assert!(!flexiglob::wildcard_match(&tok3, ".text"));

    // [^] (empty negated set) is still an error
    let err = flexiglob::compile_pattern("[^]").unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::EmptyBrackets));
}

#[test]
fn test_invalid_range_sets() {
    // Start > End range in brackets
    let tok = flexiglob::compile_pattern("[z-a]").unwrap();
    // Since z-a is invalid, it falls back to z, -, a literally.
    assert!(flexiglob::wildcard_match(&tok, "z"));
    assert!(flexiglob::wildcard_match(&tok, "-"));
    assert!(flexiglob::wildcard_match(&tok, "a"));
    assert!(!flexiglob::wildcard_match(&tok, "m"));
}

#[test]
fn test_parser_edge_cases() {
    // Empty pattern error
    let err = ParsedPattern::parse("   ", |_| false).unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::EmptyPattern));

    // '(' is reserved — no valid operator name before it is always an error.

    // Empty operator name (paren at start)
    let err1 = ParsedPattern::parse("(.text*)", |_| false).unwrap_err();
    assert!(matches!(err1.kind, ParseErrorKind::UnexpectedParen));
    assert_eq!(err1.span, 0..1);

    // Operator name starts with digit (invalid identifier)
    let err2 = ParsedPattern::parse("1SORT(.text*)", |_| false).unwrap_err();
    assert!(matches!(err2.kind, ParseErrorKind::UnexpectedParen));
    assert_eq!(err2.span, 5..6);

    // Operator name contains hyphen (invalid identifier)
    let err3 = ParsedPattern::parse("SORT-NAME(.text*)", |_| false).unwrap_err();
    assert!(matches!(err3.kind, ParseErrorKind::UnexpectedParen));
    assert_eq!(err3.span, 9..10);
}

#[test]
fn test_noop_and_default() {
    // Test default constructor
    let builder = GlobberBuilder::default();
    let globber = builder.compile(".text*").unwrap();
    let candidates = [".text".to_string()];
    let res = globber.run(&candidates, |s| s);
    assert_eq!(res.len(), 1);

}

#[test]
fn test_unregistered_but_valid_operator() {
    let builder = GlobberBuilder::<String>::new();
    let globber = builder.compile("SORT(.text*)");
    assert!(globber.is_err()); // since SORT is unregistered

    // But ParsedPattern itself can be parsed with a custom validator
    let pat = ParsedPattern::parse("SORT(.text*)", |op| op == "SORT").unwrap();
    assert_eq!(
        pat,
        ParsedPattern::Operator {
            name: "SORT".to_string(),
            inner: Box::new(leaf(".text*"))
        }
    );
}

#[test]
fn test_recursive_glob_integration() {
    let builder = GlobberBuilder::new();
    let globber = builder.compile("src/**/*.rs").unwrap();
    let candidates = vec![
        "src/lib.rs".to_string(),
        "src/parser/ast.rs".to_string(),
        "tests/integration.rs".to_string(),
    ];
    let res = globber.run(&candidates, |s| s);
    assert_eq!(res.len(), 2);
    assert_eq!(res[0], "src/lib.rs");
    assert_eq!(res[1], "src/parser/ast.rs");
}

#[test]
fn test_empty_operator_pattern_fails() {
    let builder1 = GlobberBuilder::<TargetSection>::new()
        .with_operator(SortByNameOp);
    let err = builder1.compile("SORT()").unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::EmptyPattern));

    let builder2 = GlobberBuilder::<TargetSection>::new()
        .with_operator(ReverseOp)
        .with_operator(SortByNameOp);
    let err_nested = builder2.compile("REVERSE(SORT())").unwrap_err();
    assert!(matches!(err_nested.kind, ParseErrorKind::EmptyPattern));
}

#[derive(Clone, Debug, PartialEq)]
struct IntPayload {
    value: i32,
}

struct SortIntsOp;
impl GlobOperator<IntPayload> for SortIntsOp {
    fn name(&self) -> &str {
        "SORT_INTS"
    }
    fn apply(&self, candidates: &mut Vec<&IntPayload>) {
        candidates.sort_by_key(|c| c.value);
    }
}

#[test]
fn test_no_string_payload_pipeline() {
    let builder = GlobberBuilder::new()
        .with_operator(ReverseOp)
        .with_operator(SortIntsOp);
    let globber = builder.compile("REVERSE(SORT_INTS(*))").unwrap();

    let candidates = vec![
        IntPayload { value: 30 },
        IntPayload { value: 10 },
        IntPayload { value: 20 },
    ];

    // Pass a closure returning "" for all candidates to match *
    let res = globber.run(&candidates, |_| "");

    assert_eq!(res.len(), 3);
    // SORT_INTS: 10, 20, 30
    // REVERSE: 30, 20, 10
    assert_eq!(res[0].value, 30);
    assert_eq!(res[1].value, 20);
    assert_eq!(res[2].value, 10);
}

#[test]
fn test_inline_closure_operator_integration() {
    let builder = GlobberBuilder::new()
        .with_operator(FnOperator::new("SORT_VALS", |candidates: &mut Vec<&IntPayload>| {
            candidates.sort_by_key(|c| c.value);
        }));
    let globber = builder.compile("SORT_VALS(*)").unwrap();

    let candidates = vec![
        IntPayload { value: 12 },
        IntPayload { value: 5 },
        IntPayload { value: 9 },
    ];

    let res = globber.run(&candidates, |_| "");
    assert_eq!(res.len(), 3);
    assert_eq!(res[0].value, 5);
    assert_eq!(res[1].value, 9);
    assert_eq!(res[2].value, 12);
}

// --- Empty bracket set ---

#[test]
fn test_empty_bracket_set_is_an_error() {
    // [] can never match anything, so it is a compile-time error rather than a
    // silent always-false token.

    // Standalone: '[' at byte 0, ']' at byte 1, span covers both.
    let err = flexiglob::compile_pattern("[]").unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::EmptyBrackets));
    assert_eq!(err.span, 0..2);

    // Mid-pattern: "foo" = 3 bytes, so '[' at byte 3, ']' at byte 4.
    let err2 = flexiglob::compile_pattern("foo[]bar").unwrap_err();
    assert!(matches!(err2.kind, ParseErrorKind::EmptyBrackets));
    assert_eq!(err2.span, 3..5);

    // Byte-accurate span with a multi-byte prefix:
    // 'ä' (U+00E4) = 2 bytes, so '[' at byte 2, ']' at byte 3.
    let err3 = flexiglob::compile_pattern("ä[]").unwrap_err();
    assert!(matches!(err3.kind, ParseErrorKind::EmptyBrackets));
    assert_eq!(err3.span, 2..4);
}

// --- Multi-byte (Unicode) character tests ---

#[test]
fn test_multibyte_char_matching() {
    // Literal multi-byte chars in a pattern match correctly against candidates.
    // 'é' is U+00E9 = 2 UTF-8 bytes.
    let tok = flexiglob::compile_pattern("café*").unwrap();
    assert!(flexiglob::wildcard_match(&tok, "café_main"));
    assert!(flexiglob::wildcard_match(&tok, "café"));
    assert!(!flexiglob::wildcard_match(&tok, "cafe_main")); // ASCII 'e', not 'é'
    assert!(!flexiglob::wildcard_match(&tok, "bar"));

    // '?' matches one Unicode codepoint, not one byte.
    // 'é' is one codepoint (two bytes), so "caf?" must accept "café".
    let tok2 = flexiglob::compile_pattern("caf?_section").unwrap();
    assert!(flexiglob::wildcard_match(&tok2, "café_section"));
    assert!(flexiglob::wildcard_match(&tok2, "cafx_section"));
    assert!(!flexiglob::wildcard_match(&tok2, "café_section_extra"));

    // '*' matches across multi-byte chars in the candidate.
    let tok3 = flexiglob::compile_pattern("*é*").unwrap();
    assert!(flexiglob::wildcard_match(&tok3, "café_main"));
    assert!(flexiglob::wildcard_match(&tok3, "é"));
    assert!(!flexiglob::wildcard_match(&tok3, "cafe_main")); // no 'é'

    // Character sets containing multi-byte chars.
    let tok4 = flexiglob::compile_pattern("[éàü]_section").unwrap();
    assert!(flexiglob::wildcard_match(&tok4, "é_section"));
    assert!(flexiglob::wildcard_match(&tok4, "à_section"));
    assert!(flexiglob::wildcard_match(&tok4, "ü_section"));
    assert!(!flexiglob::wildcard_match(&tok4, "e_section"));
    assert!(!flexiglob::wildcard_match(&tok4, "ä_section"));
}

#[test]
fn test_multibyte_char_error_spans() {
    // Error spans must be byte offsets, not char offsets.
    // 'ä' is U+00E4 = 2 UTF-8 bytes, so all chars after it shift by 1 byte
    // relative to their char index.

    // "ä[abc": ä=bytes 0-1, [=byte 2, a=3, b=4, c=5 → pattern.len()=6
    // '[' lives at byte 2, so the unterminated-bracket span must start at 2.
    let err = flexiglob::compile_pattern("ä[abc").unwrap_err();
    assert!(matches!(err.kind, ParseErrorKind::UnterminatedBracketSet));
    assert_eq!(err.span, 2..6);

    // "ä\": ä=bytes 0-1, \=byte 2 → pattern.len()=3
    // Dangling backslash lives at byte 2.
    let err2 = flexiglob::compile_pattern("ä\\").unwrap_err();
    assert!(matches!(err2.kind, ParseErrorKind::UnexpectedTrailingCharacters));
    assert_eq!(err2.span, 2..3);
}

#[test]
fn test_multibyte_inner_pattern_not_truncated() {
    // Verifies that multi-byte chars inside an operator's argument are parsed
    // without truncation.
    //
    // "SORT(ä.text*)" byte layout:
    //   S=0, O=1, R=2, T=3, (=4, ä=5-6, .=7, t=8, e=9, x=10, t=11, *=12, )=13
    //
    // The old code stored close_idx as a char index (12) and used it as a byte
    // offset, slicing trimmed[5..12] = "ä.text" — silently dropping the '*'.
    // The new code stores close_byte as byte offset 13, giving trimmed[5..13]
    // = "ä.text*" (correct).
    let builder = GlobberBuilder::new()
        .with_operator(SortByNameOp);
    let globber = builder.compile("SORT(ä.text*)").unwrap();

    let candidates = vec![
        TargetSection { file: "a.elf".to_string(), name: "ä.text_init".to_string(), align: 4, size: 32 },
        TargetSection { file: "b.elf".to_string(), name: "ä.text_main".to_string(), align: 4, size: 32 },
        TargetSection { file: "c.elf".to_string(), name: ".data".to_string(), align: 4, size: 32 },
    ];

    let result = globber.run(&candidates, |s| &s.name);

    // Both ä.text_* sections must match; .data must not.
    assert_eq!(result.len(), 2);
    // SORT orders alphabetically: ä.text_init before ä.text_main.
    assert_eq!(result[0].name, "ä.text_init");
    assert_eq!(result[1].name, "ä.text_main");
}
