#![no_main]

use flexiglob::{FnOperator, GlobberBuilder, ReverseOp};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = core::str::from_utf8(data) else { return };
    let mut lines = s.lines();
    let Some(pattern) = lines.next() else { return };
    let candidates: Vec<String> = lines.map(str::to_string).collect();

    let builder = GlobberBuilder::new()
        .with_operator(ReverseOp)
        .with_operator(FnOperator::new("SORT", |v: &mut Vec<&String>| v.sort()))
        .with_operator(FnOperator::new("FILTER", |v: &mut Vec<&String>| {
            v.retain(|s| !s.is_empty())
        }));

    if let Ok(globber) = builder.compile(pattern) {
        let _ = globber.run(&candidates, |s| s.as_str());
    }
});
