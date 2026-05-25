# Flexiglob

Flexiglob is a versatile wildcard query engine for Rust. Flexiglob parses
nested, functional pattern expressions, such as `REVERSE(SORT(.text*))`, and
matches these expressions against arbitrary user-defined data structures.

Though inspired by [GNU linker wildcard
matching](https://sourceware.org/binutils/docs/ld/Input-Section-Wildcards.html),
Flexiglob operates without coupling to host filesystem or binary object formats.
Instead, Flexiglob delegates property extraction to caller-supplied closures and
through an extensible operator registry.

---

## Features

- **Wildcard Pattern Matching**: Supports shell-like and path-aware wildcards
  such as `*`, `**`, `?`, bracket sets `[chars]`, character ranges such as
  `a-z`, and explicit backslash escaping.
- **Custom Nestable Operators**: Supports use and nesting of user-defined
  operators to sort and filter matching data sets, e.g.
  `REVERSE(MY_FILTER(SORT(foo*)))`.
- **First-Class Error Diagnostics**: Errors include precise character byte spans
  relative to the input pattern string, enabling rich diagnostic messages.
- **Lightweight & Portable**: Fully `#![no_std]` compatible, dependency-free,
  and suitable for bare-metal, embedded, WebAssembly, or standard application
  use.
- **Well Bounded Execution**: Uses non-deterministic finite automata (NFA)
  simulation for worst-case time complexity O(n×m), where n is the token count
  and m is the candidate string length. Worst-case space complexity is O(n) to
  store the NFA states.

---

## Installation

In your Rust project, run:

    cargo add flexiglob

## Example

The example below uses Flexiglob to match all `.text*` sections from a vector of
input strings.  The code implements a custom operator `SORT_SIZE` to sort the
sections by a size property.

```rust
use flexiglob::{GlobberBuilder, GlobOperator};

// 1. Define candidate structure
#[derive(Clone, Debug)]
struct Section {
    name: String,
    size: u64,
}

// 2. Define custom operator
struct SortBySize;
impl GlobOperator<Section> for SortBySize {
    fn name(&self) -> &str { "SORT_SIZE" }
    fn apply(&self, candidates: &mut Vec<Section>) {
        candidates.sort_by_key(|s| s.size);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 3. Register operators and compile pattern
    let pattern_str = "SORT_SIZE(.text*)";
    let builder = GlobberBuilder::new()
        .with_operator(SortBySize);
    let globber = builder.compile(pattern_str)?;

    // 4. Evaluate matching
    let candidates = vec![
        Section { name: ".text.boot".to_string(), size: 100 },
        Section { name: ".text.main".to_string(), size: 500 },
        Section { name: ".data.debug".to_string(), size: 200 },
    ];

    let results = globber.run(&candidates, |s| &s.name);

    // Result: [.text.boot (size 100), .text.main (size 500)]
    println!("{:?}", results);
    Ok(())
}
```

---

## Pattern Matching Reference

Flexiglob supports wildcards and escaping rules matching the following syntax:

| Pattern      | Description                                                    |
| ------------ | -------------------------------------------------------------- |
| `*`          | Match 0 or more characters, excluding `/` file path separators |
| `**`         | Match 0 or more characters, including `/` file path separators |
| `?`          | Match exactly one character                                    |
| `[<chars>]`  | Match any single character in the specified set                |
| `[^<chars>]` | Match any single non-separator character not in the set        |
| `[a-z]`      | Match any character in the ASCII range `a` to `z`              |
| `\*`         | Match a literal `*` character                                  |
| `\?`         | Match a literal `?` character                                  |
| `\[`         | Match a literal `[` character                                  |
| `\]`         | Match a literal `]` character                                  |
| `\"`         | Match a literal `"` character                                  |
| `\\`         | Match a literal `\` character                                  |

---

## Extending Flexiglob

Users can easily extend Flexiglob patterns with their own operators of the form
`MY_OPERATOR(...)`.  These operators wrap an expression to filter or manipulate
the in-flight result.  For example, Flexiglob comes with `REVERSE` which simply
reverses the order of the result:

    // Input: s1abc, s2def, s3ghi
    "REVERSE(stuff[12]*)"
    // Resulting match output: s2def, s1abc

To allow users to register and execute custom operators:

1. Implement the `GlobOperator` trait for the candidate type `T`.
2. Register the operator instance in `GlobberBuilder` using `with_operator`.
3. Call `compile` to parse the pattern string and validate operator names,
   producing a `Globber`.
4. Call `run` on the compiled `Globber` with candidates.

As a built-in example, Flexiglob provides the `REVERSE()` operator defined as follows:

```rust
pub struct ReverseOp;

impl<T> GlobOperator<T> for ReverseOp {
    fn name(&self) -> &str { "REVERSE" }
    fn apply(&self, candidates: &mut Vec<T>) {
        candidates.reverse();
    }
}
```

---

## Error Reporting

On syntax or validation failures, `GlobberBuilder::compile` returns a `ParseError` structure:

```rust
pub struct ParseError {
    /// The type of syntax error that occurred.
    pub kind: ParseErrorKind,

    /// The span of the offending characters relative to the input string start.
    pub span: Range<usize>,

    /// An explanation of the error.
    pub message: String,
}
```

Users can relay this information to an error formatting library such as
[`ariadne`](https://crates.io/crates/ariadne).  For example, here's the
`ariadne` output from `examples/error_reporting`:

```bash
$ cargo run --example error_reporting

--- Testing Pattern: 'REVERSE(SORT(.text*' ---
Error: Syntax error in pattern expression
   ╭─[config.firmion:1:8]
   │
 1 │ REVERSE(SORT(.text*
   │        ┬
   │        ╰── Missing closing parenthesis for operator
───╯
--- Testing Pattern: 'REVERSE(SORTR(.text*))' ---
Error: Syntax error in pattern expression
   ╭─[config.firmion:1:9]
   │
 1 │ REVERSE(SORTR(.text*))
   │         ──┬──
   │           ╰──── Invalid or unrecognized operator name 'SORTR'
───╯
--- Testing Pattern: 'SORT(.text[a-z)' ---
Error: Syntax error in pattern expression
   ╭─[config.firmion:1:11]
   │
 1 │ SORT(.text[a-z)
   │           ──┬─
   │             ╰─── Unterminated bracket set
───╯
```
