use flexiglob::{GlobberBuilder, GlobOperator, ReverseOp};
use ariadne::{Color, Label, Report, ReportKind, Source};

struct SortOp;
impl<T> GlobOperator<T> for SortOp {
    fn name(&self) -> &str {
        "SORT"
    }
    fn apply(&self, _candidates: &mut Vec<T>) {}
}

fn print_diagnostic(pattern: &str, file_name: &str, error: &flexiglob::ParseError) {
    // Generate the ariadne report mapping the relative span to the source string
    Report::build(ReportKind::Error, file_name, error.span.start)
        .with_message("Syntax error in pattern expression")
        .with_label(
            Label::new((file_name, error.span.clone()))
                .with_message(&error.message)
                .with_color(Color::Red),
        )
        .finish()
        .print((file_name, Source::from(pattern)))
        .unwrap();
}

fn main() {
    let file_name = "config.firmion";
    let builder = GlobberBuilder::<String>::new()
        .with_operator(ReverseOp)
        .with_operator(SortOp);

    // Test Case 1: Mismatched parentheses
    {
        let pattern = "REVERSE(SORT(.text*";
        println!("--- Testing Pattern: '{}' ---", pattern);
        if let Err(e) = builder.compile(pattern) {
            print_diagnostic(pattern, file_name, &e);
        }
        println!();
    }

    // Test Case 2: Unrecognized operator name (typo)
    {
        let pattern = "REVERSE(SORTR(.text*))";
        println!("--- Testing Pattern: '{}' ---", pattern);
        let builder = GlobberBuilder::<String>::new().with_operator(ReverseOp).with_operator(SortOp);
        if let Err(e) = builder.compile(pattern) {
            print_diagnostic(pattern, file_name, &e);
        }
        println!();
    }

    // Test Case 3: Unterminated bracket set
    {
        let pattern = "SORT(.text[a-z)";
        println!("--- Testing Pattern: '{}' ---", pattern);
        let builder = GlobberBuilder::<String>::new().with_operator(SortOp);
        if let Err(e) = builder.compile(pattern) {
            print_diagnostic(pattern, file_name, &e);
        }
        println!();
    }

    // Test Case 4: Unexpected trailing characters
    {
        let pattern = "REVERSE(.text*)extra";
        println!("--- Testing Pattern: '{}' ---", pattern);
        let builder = GlobberBuilder::<String>::new().with_operator(ReverseOp).with_operator(SortOp);
        if let Err(e) = builder.compile(pattern) {
            print_diagnostic(pattern, file_name, &e);
        }
        println!();
    }
}
