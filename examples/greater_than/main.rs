use flexiglob::{GlobberBuilder, GlobOperator};

// Define the data structure to match against.
#[derive(Clone, Debug)]
struct Section {
    name: String,
    size: u64,
}

// A custom operator that holds a configured threshold.
// This demonstrates how struct-based operators can carry state or configuration.
struct FilterGreaterThan {
    min_size: u64,
}

impl GlobOperator<Section> for FilterGreaterThan {
    fn name(&self) -> &str {
        "BIGGER_THAN"
    }

    fn apply(&self, candidates: &mut Vec<Section>) {
        candidates.retain(|s| s.size > self.min_size);
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

    // Configure the filter operator with a threshold of 150 bytes.
    let filter_op = FilterGreaterThan { min_size: 150 };

    let pattern_str = "BIGGER_THAN(.text*)";
    let globber = GlobberBuilder::new()
        .with_operator(filter_op)
        .compile(pattern_str)?;

    println!("\nEvaluating pattern: {}", pattern_str);
    let results = globber.run(&candidates, |s| &s.name);

    println!("Matched and filtered results (size > 150):");
    for r in &results {
        println!("  Name: {}, Size: {}", r.name, r.size);
    }

    Ok(())
}
