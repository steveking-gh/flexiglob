// Filesystem traversal for the optional `fs` feature.  Adds run_fs to
// Globber<'_, String>, which uses scan_hint to determine the root directory,
// walks the filesystem to build a candidate list, then runs the normal match
// pipeline.  Directory symlink cycles are detected by tracking canonicalized
// paths; broken symlinks are silently skipped.  All returned paths use forward
// slashes regardless of platform.

use crate::globber::{Globber, ScanHint};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

impl<'a> Globber<'a, String> {
    /// Evaluates the compiled pattern against the local filesystem.
    ///
    /// Uses `scan_hint()` to determine the root to scan, enumerates candidates
    /// from the filesystem, then runs the full match-and-operator pipeline.
    /// All returned paths use forward slashes regardless of platform.
    pub fn run_fs(&self) -> Vec<String> {
        let hint = self.scan_hint();
        let candidates = enumerate_candidates(&hint);
        self.run(&candidates, |s| s.as_str())
            .into_iter()
            .cloned()
            .collect()
    }
}

fn enumerate_candidates(hint: &ScanHint<'_>) -> Vec<String> {
    let root = if hint.root.is_empty() { "." } else { hint.root };
    let mut candidates = Vec::new();

    if hint.is_literal {
        // Skip traversal: return the path as-is so the caller's I/O layer
        // produces a precise OS error if the file is absent, rather than
        // silently returning no matches.
        candidates.push(normalize(root));
    } else {
        let root_path = Path::new(root);
        if root_path.is_dir() {
            let mut visited = HashSet::new();
            if let Ok(canonical) = root_path.canonicalize() {
                visited.insert(canonical);
            }
            collect(root_path, hint.is_recursive, &mut visited, &mut candidates);
            candidates.sort();
        }
    }
    candidates
}

fn normalize(p: &str) -> String {
    p.replace('\\', "/")
}

fn collect(dir: &Path, recursive: bool, visited: &mut HashSet<PathBuf>, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        // metadata() follows symlinks: symlinks to files/dirs are treated as
        // their target type; broken symlinks are silently skipped.
        let Ok(meta) = fs::metadata(&path) else { continue };

        if meta.is_file() {
            if let Some(s) = path.to_str() {
                out.push(normalize(s));
            }
        } else if meta.is_dir() && recursive {
            // canonicalize() resolves the real path behind any symlinks.
            // If this directory was already visited we have a cycle; skip it.
            if let Ok(canonical) = path.canonicalize() {
                if visited.insert(canonical) {
                    collect(&path, recursive, visited, out);
                }
            }
        }
    }
}
