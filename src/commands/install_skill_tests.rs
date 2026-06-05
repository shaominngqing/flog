use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;
use crate::cli::SkillAgent;

#[test]
fn install_all_writes_common_skill_and_agent_adapters() {
    let dir = temp_project("all");

    let report = install_for_project(&dir, SkillAgent::All).unwrap();

    assert!(dir.join(".flog/skills/flog-inspect/SKILL.md").exists());
    assert!(dir.join("AGENTS.md").exists());
    assert!(dir.join("CLAUDE.md").exists());
    assert!(dir.join(".cursor/rules/flog-inspect.mdc").exists());
    assert_eq!(report.files.len(), 4);
    assert_contains(
        &dir.join("AGENTS.md"),
        "Use `.flog/skills/flog-inspect/SKILL.md`",
    );
    assert_contains(&dir.join("CLAUDE.md"), "@AGENTS.md");
    assert_contains(
        &dir.join(".cursor/rules/flog-inspect.mdc"),
        "alwaysApply: false",
    );
    assert_contains(
        &dir.join(".cursor/rules/flog-inspect.mdc"),
        ".flog/skills/flog-inspect/SKILL.md",
    );
    let claude = fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
    assert!(!claude.contains("flog ai snapshot"));
    let cursor = fs::read_to_string(dir.join(".cursor/rules/flog-inspect.mdc")).unwrap();
    assert!(!cursor.contains("flog ai get net#42 --detail"));
}

#[test]
fn install_replaces_managed_block_without_touching_user_content() {
    let dir = temp_project("replace");
    fs::write(
        dir.join("AGENTS.md"),
        "Keep this.\n<!-- flog-inspect:start -->\nold\n<!-- flog-inspect:end -->\n",
    )
    .unwrap();

    install_for_project(&dir, SkillAgent::Codex).unwrap();
    install_for_project(&dir, SkillAgent::Codex).unwrap();

    let content = fs::read_to_string(dir.join("AGENTS.md")).unwrap();
    assert!(content.contains("Keep this."));
    assert!(!content.contains("\nold\n"));
    assert_eq!(content.matches("<!-- flog-inspect:start -->").count(), 1);
}

#[test]
fn install_cursor_only_skips_codex_and_claude_adapters() {
    let dir = temp_project("cursor");

    install_for_project(&dir, SkillAgent::Cursor).unwrap();

    assert!(dir.join(".flog/skills/flog-inspect/SKILL.md").exists());
    assert!(dir.join(".cursor/rules/flog-inspect.mdc").exists());
    assert!(!dir.join("AGENTS.md").exists());
    assert!(!dir.join("CLAUDE.md").exists());
}

fn temp_project(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("flog-install-skill-{label}-{unique}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn assert_contains(path: &Path, expected: &str) {
    let content = fs::read_to_string(path).unwrap();
    assert!(
        content.contains(expected),
        "expected {path:?} to contain {expected:?}, got {content:?}"
    );
}
