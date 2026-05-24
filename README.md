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
- **Optional Tracing Callback**: Inspect intermediate matching states at each
  pipeline phase via a simple closure hook.
- **Lightweight & Portable**: Fully `#![no_std]` compatible, dependency-free,
  and suitable for bare-metal, embedded, WebAssembly, or standard application
  use.
- **Well Bounded Execution**: Uses non-deterministic finite automata (NFA)
  simulation for worst-case time complexity O(n×m), where n is the token count
  and m is the candidate string length. Worst-Case space complexity is O(n) to
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
    let globber = GlobberBuilder::new()
        .with_operator(SortBySize)
        .compile(pattern_str)?;

    // 4. Evaluate matching
    let candidates = vec![
        Section { name: ".text.boot".to_string(), size: 100 },
        Section { name: ".text.main".to_string(), size: 500 },
        Section { name: ".data.debug".to_string(), size: 200 },
    ];

    let results = globber.run(&candidates, |s| &s.name);

    // Result: [.text.main (size 500), .text.boot (size 100)]
    println!("{:?}", results);
    Ok(())
}
```

---

## Pattern Matching Reference

Flexiglob supports wildcards and escaping rules matching the following syntax:

| Pattern     | Description                                                    |
| ----------- | -------------------------------------------------------------- |
| `*`         | Match 0 or more characters, excluding `/` file path separators |
| `**`        | Match 0 or more characters, including `/` file path separators |
| `?`         | Match exactly one character                                    |
| `[<chars>]` | Match any single character in the specified set                |
| `[a-z]`     | Match any character in the ASCII range `a` to `z`              |
| `\*`        | Match a literal `*` character                                  |
| `\?`        | Match a literal `?` character                                  |
| `\[`        | Match a literal `[` character                                  |
| `\]`        | Match a literal `]` character                                  |
| `\"`        | Match a literal `"` character                                  |
| `\\`        | Match a literal `\` character                                  |

---

## Extending Flexiglob

To allow users to register and execute custom operators:
1. Implement the [GlobOperator](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L375) trait for the candidate type `T`.
2. Register the operator instance in the [GlobberBuilder](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L390) context using `with_operator`.
3. Call `compile` to parse the pattern string and validate command names, building the [Globber](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L429) instance.
4. Call `run` on the compiled `Globber` with candidates.

---

## Error Reporting

On syntax or validation failures,
[GlobberBuilder::compile](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L420)
returns a
[ParseError](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L54)
structure:

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


### Interoperating with Ariadne
Users can translate this relative span into an absolute file location for underline-annotated terminal diagnostic outputs:

```rust
let absolute_start = literal_offset + error.span.start;
let absolute_end = literal_offset + error.span.end;

Report::build(ReportKind::Error, file_id, absolute_start)
    .with_message("Syntax error in pattern")
    .with_label(Label::new((file_id, absolute_start..absolute_end)).with_message(&error.message))
    .finish()
    .print(sources)?;
```

---

## Execution Tracing

Users can log execution progress using `run_with_trace` by providing a [TraceCallback](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L384):

```rust
let trace_cb = |msg: &str, items: &[Section]| {
    println!("{}: {:?}", msg, items);
};

globber.run_with_trace(&candidates, |s| &s.name, &trace_cb);
```

To run silently without tracing, callers can use `run` which internally routes tracing to the no-op fallback [noop_trace](file:///c:/Users/kings/Documents/projects/flexiglob/src/lib.rs#L387).
