// NFA-based wildcard matching engine.  Given a compiled token slice produced by
// parser::compile_pattern and a candidate string, wildcard_match runs a
// non-deterministic finite automaton simulation that matches in O(n×m) time
// with O(n) space — no backtracking, no exponential worst cases.  The only
// recognised path separator is '/'; backslash in candidate strings is treated
// as an ordinary character.

use crate::parser::MatchToken;

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
/// Token              Condition     Adds to next
/// -----------------  ------------  ----------------
/// Char(ch)           c == ch       i+1
/// AnyChar            c != '/'      i+1
/// Set(s)             c in s        i+1
/// AnySeqNoSeparator  c != '/'      i  (self-loop)
/// AnySeq             any c         i  (self-loop)
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
///   i --eps--> i+2        (only when tokens[i+1] is '/')
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::compile_pattern;

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
