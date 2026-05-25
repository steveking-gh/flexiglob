use flexiglob::{GlobberBuilder, GlobOperator, ReverseOp};

#[derive(Clone, Debug)]
struct Section {
    name: String,
    size: u64,
}

struct SortBySize;

impl GlobOperator<Section> for SortBySize {
    fn name(&self) -> &str { "SORT_SIZE" }
    fn apply(&self, candidates: &mut Vec<Section>) {
        candidates.sort_by_key(|s| s.size);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let candidates = vec![
        Section { name: ".text.boot".to_string(), size: 300 },
        Section { name: ".text.init".to_string(), size: 100 },
        Section { name: ".text.main".to_string(), size: 500 },
        Section { name: ".data.bss".to_string(),  size: 200 },
    ];

    let pattern_str = "REVERSE(SORT_SIZE(.text*))";
    let builder = GlobberBuilder::new()
        .with_operator(ReverseOp)
        .with_operator(SortBySize);
    let globber = builder.compile(pattern_str)?;

    println!("Pattern: {}\n", pattern_str);

    let trace = |label: &str, items: &[Section]| {
        println!("[trace] {}:", label);
        for s in items {
            println!("  {} (size {})", s.name, s.size);
        }
        println!();
    };

    let results = globber.run_with_trace(&candidates, |s| &s.name, &trace);

    println!("Final results:");
    for s in &results {
        println!("  {} (size {})", s.name, s.size);
    }

    Ok(())
}
