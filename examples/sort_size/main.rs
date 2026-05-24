use flexiglob::{FnOperator, GlobOperator, GlobberBuilder};

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
// This is the traditional approach. It allows the operator to hold configuration
// parameters or state inside struct fields.
struct SortBySize;

impl GlobOperator<Section> for SortBySize {
    fn name(&self) -> &str {
        "SORT_SIZE"
    }

    fn apply(&self, candidates: &mut Vec<Section>) {
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
        let globber = GlobberBuilder::new()
            .with_operator(SortBySize)
            .compile(pattern_str)?;

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
        let globber = GlobberBuilder::new()
            .with_operator(FnOperator::new("INLINE_SORT_SIZE", |candidates: &mut Vec<Section>| {
                candidates.sort_by_key(|s| s.size);
            }))
            .compile(pattern_str)?;

        println!("\nEvaluating pattern (Inline Closure-based): {}", pattern_str);
        let results = globber.run(&candidates, |s| &s.name);

        println!("Matched and sorted results:");
        for r in &results {
            println!("  Name: {}, Size: {}", r.name, r.size);
        }
    }

    Ok(())
}
