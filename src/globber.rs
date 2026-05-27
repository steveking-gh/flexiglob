// The primary public API surface of flexiglob.  GlobberBuilder holds the
// operator registry and compiles pattern strings into Globber instances.
// Globber runs the compiled pattern against a caller-supplied candidate slice,
// applying any registered operators in pipeline order.  ScanHint is a
// lightweight struct returned by Globber::scan_hint that tells callers the
// minimum filesystem root and traversal depth needed to build a complete
// candidate set for a given pattern.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::matcher::wildcard_match;
use crate::operator::GlobOperator;
use crate::parser::{MatchToken, ParsedPattern, ParseError, ParseErrorKind};

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
    ///
    /// Panics if an operator with the same name is already registered.
    /// Use [`register_operator`](Self::register_operator) for dynamic registration
    /// where a `Result` is preferable to a panic.
    pub fn with_operator(mut self, op: impl GlobOperator<T> + 'a) -> Self {
        self.register_operator(op)
            .expect("operator name already registered");
        self
    }

    /// Registers a custom pipeline operator.
    ///
    /// Returns `Err` if an operator with the same name is already registered.
    pub fn register_operator(&mut self, op: impl GlobOperator<T> + 'a) -> Result<(), ParseError> {
        let name = op.name().to_string();
        if self.operators.contains_key(&name) {
            return Err(ParseError {
                kind: ParseErrorKind::DuplicateOperator(name.clone()),
                span: 0..0,
                message: alloc::format!("operator '{}' is already registered", name),
            });
        }
        self.operators.insert(name, Box::new(op));
        Ok(())
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

/// The compiled matching and operator execution engine.
pub struct Globber<'a, T> {
    pub(crate) pattern: ParsedPattern,
    pub(crate) operators: &'a BTreeMap<String, Box<dyn GlobOperator<T> + 'a>>,
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
    /// src/foo\*.rs             "src/foo/"            false      true
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operator::{FnOperator, GlobOperator, ReverseOp};
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

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
}
