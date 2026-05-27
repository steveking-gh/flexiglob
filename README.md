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
use flexiglob::{FnOperator, GlobOperator, GlobberBuilder, ReverseOp};

// Define the data structure to match against. While fully under user control,
// this structure should have at least a string field. The closure passed to
// `globber.run` extracts the string from this structure.
#[derive(Clone, Debug)]
struct Section {
    // The string to match against.
    name: String,
    // An additional custom field to sort by.
    size: u64,
}

// Style A: Struct-Based Custom Operator
// This is the traditional approach which allows the operator to hold configuration
// parameters or state inside struct fields.
struct SortBySize;

impl GlobOperator<Section> for SortBySize {
    fn name(&self) -> &str {
        "SORT_SIZE"
    }

    fn apply(&self, candidates: &mut Vec<&Section>) {
        candidates.sort_by_key(|s| s.size);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let candidates = vec![
        Section {
            name: ".text.boot".to_string(),
            size: 100,
        },
        Section {
            name: ".text.main".to_string(),
            size: 500,
        },
        Section {
            name: ".data.debug".to_string(),
            size: 200,
        },
    ];

    println!("Input candidates:");
    for c in &candidates {
        println!("  Name: {}, Size: {}", c.name, c.size);
    }

    // Style A: Struct-Based Custom Operator "SORT_SIZE".  We also REVERSE()
    // the sorted list to demonstrate operator nesting.
    {
        let pattern_str = "REVERSE(SORT_SIZE(.text*))";
        let builder = GlobberBuilder::new()
            .with_operator(ReverseOp)
            .with_operator(SortBySize);
        let globber = builder.compile(pattern_str)?;

        println!("\nEvaluating pattern (Struct-based): {}", pattern_str);
        let results = globber.run(&candidates, |s| &s.name);

        println!("Matched and sorted results:");
        for r in &results {
            println!("  Name: {}, Size: {}", r.name, r.size);
        }
    }

    // Style B: Inline Closure-Based Custom Operator "INLINE_SORT_SIZE".  This
    // allows registering custom operators inline without defining a new struct.
    {
        let pattern_str = "INLINE_SORT_SIZE(.text*)";
        let builder = GlobberBuilder::new()
            .with_operator(FnOperator::new("INLINE_SORT_SIZE", |candidates: &mut Vec<&Section>| {
                candidates.sort_by_key(|s| s.size);
            }));
        let globber = builder.compile(pattern_str)?;

        println!("\nEvaluating pattern (Inline Closure-based): {}", pattern_str);
        let results = globber.run(&candidates, |s| &s.name);

        println!("Matched and sorted results:");
        for r in &results {
            println!("  Name: {}, Size: {}", r.name, r.size);
        }
    }

    Ok(())
}
```

Produces the following output:

    $ cargo run --example sort_size
        Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.06s
        Running `target\debug\examples\sort_size.exe`
    Input candidates:
      Name: .text.boot, Size: 100
      Name: .text.main, Size: 500
      Name: .data.debug, Size: 200

    Evaluating pattern (Struct-based): REVERSE(SORT_SIZE(.text*))
    Matched and sorted results:
      Name: .text.main, Size: 500
      Name: .text.boot, Size: 100

    Evaluating pattern (Inline Closure-based): INLINE_SORT_SIZE(.text*)
    Matched and sorted results:
      Name: .text.boot, Size: 100
      Name: .text.main, Size: 500

---

## Pattern Matching Reference

Flexiglob supports wildcards and escaping rules matching the following syntax:

| Pattern      | Description                                                     |
| ------------ | --------------------------------------------------------------- |
| `*`          | Match 0 or more characters, excluding `/` file path separators.<br>This form does *not* match files in subdirectories. |
| `**`         | Match 0 or more characters, including `/` file path separators.<br>This form does match files in subdirectories.  |
| `?`          | Match exactly one character, excluding `/` file path separators |
| `[<chars>]`  | Match any single character in the specified set                 |
| `[^<chars>]` | Match any single non-separator character not in the set         |
| `[a-z]`      | Match any character in the ASCII range `a` to `z`               |
| `\*`         | Match a literal `*` character                                   |
| `\?`         | Match a literal `?` character                                   |
| `\[`         | Match a literal `[` character                                   |
| `\]`         | Match a literal `]` character                                   |
| `\(`         | Match a literal `(` character                                   |
| `\)`         | Match a literal `)` character                                   |
| `\"`         | Match a literal `"` character                                   |
| `\"`         | Match a literal `"` character                                   |
| `\\`         | Match a literal `\` character                                   |

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
    fn apply(&self, candidates: &mut Vec<&T>) {
        candidates.reverse();
    }
}
```

---

## Filesystem Traversal Hints

Users can optionally call `scan_hint()` on a compiled `Globber` to aid in
building the input set.

    let hint = globber.scan_hint();

When matching files, `ScanHint` returns the filesystem traversal needed to build
a complete candidate set for this pattern.

The members of `scan_hint` result are as follows:

- `root` - Longest match string before the first wildcard
- `is_recursive` - true if the pattern contains ** and needs recursive traversal
  for file system globs.
- `is_literal` - true if the pattern contains no wildcards at all.

The `scan_hint` `root` result ignores operator wrappers around the wildcard
string.  For example, the `root` result contains `some/path/` for all of these
glob strings:

     "some/path/*.elf"
     "SORT(some/path/*.elf)"
     "REVERSE(SORT(some/path/*.elf))"

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

## Fuzz Testing

Flexiglob supports fuzz testing which tries to induce panics in the flexiglob library.  Running fuzz tests requires a Linux development environment.  To run the fuzzer, use

    cargo +nightly fuzz run fuzz_target -- -dict=tests/flexiglob_fuzz.dict