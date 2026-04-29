//! CI guardrail tests for patterns banned in production source.
//!
//! Failure means someone reintroduced a pattern the team has already
//! decided against. Fix the source, not this test.

use std::path::{Path, PathBuf};

/// Recursively walk `root`, returning paths of all .rs files (skipping
/// `target/` and hidden directories). Pure std::fs — no walkdir dep.
fn collect_rs_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with('.') || name == "target" {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }
    out
}

fn scan_production(pattern: &str) -> Vec<String> {
    let mut offenders = Vec::new();
    for p in collect_rs_files(Path::new("src")) {
        let s = p.to_string_lossy();
        if s.ends_with("_tests.rs") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&p) else {
            continue;
        };
        for (i, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") || trimmed.starts_with("*") {
                continue;
            }
            if line.contains(pattern) {
                offenders.push(format!("  {}:{}  {}", p.display(), i + 1, line.trim()));
            }
        }
    }
    offenders
}

#[test]
fn no_eprintln_in_production_code() {
    let offenders = scan_production("eprintln!");
    assert!(
        offenders.is_empty(),
        "\neprintln! in production code pollutes alternate screen \
         (stderr bypasses EnterAlternateScreen):\n{}\n",
        offenders.join("\n")
    );
}
