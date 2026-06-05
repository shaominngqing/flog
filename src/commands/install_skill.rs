use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::cli::{InstallSkillArgs, SkillAgent};

const SKILL_MD: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/skills/flog-inspect/SKILL.md"
));
const BEGIN: &str = "<!-- flog-inspect:start -->";
const END: &str = "<!-- flog-inspect:end -->";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallReport {
    pub files: Vec<InstalledFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFile {
    pub path: PathBuf,
    pub action: InstallAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallAction {
    Created,
    Updated,
    Unchanged,
}

pub fn run(args: InstallSkillArgs) -> io::Result<()> {
    let report = install_for_project(&args.project, args.agent)?;
    println!("Installed flog inspect guidance:");
    for file in report.files {
        println!("  - {:?}: {}", file.path, action_label(file.action));
    }
    println!();
    println!("Next: ask your AI agent to inspect the app with flog, or run `flog ai snapshot --last 20 --net-last 20`.");
    Ok(())
}

pub fn install_for_project(project: &Path, agent: SkillAgent) -> io::Result<InstallReport> {
    fs::create_dir_all(project)?;
    let mut files = Vec::new();

    files.push(write_file_if_changed(
        &project.join(".flog/skills/flog-inspect/SKILL.md"),
        SKILL_MD,
    )?);

    if installs_codex(agent) {
        files.push(write_managed_markdown(
            &project.join("AGENTS.md"),
            &agent_block(),
        )?);
    }
    if installs_claude(agent) {
        files.push(write_managed_markdown(
            &project.join("CLAUDE.md"),
            &claude_block(),
        )?);
    }
    if installs_cursor(agent) {
        files.push(write_file_if_changed(
            &project.join(".cursor/rules/flog-inspect.mdc"),
            &cursor_rule(),
        )?);
    }

    Ok(InstallReport { files })
}

fn installs_codex(agent: SkillAgent) -> bool {
    matches!(agent, SkillAgent::All | SkillAgent::Codex)
}

fn installs_claude(agent: SkillAgent) -> bool {
    matches!(agent, SkillAgent::All | SkillAgent::Claude)
}

fn installs_cursor(agent: SkillAgent) -> bool {
    matches!(agent, SkillAgent::All | SkillAgent::Cursor)
}

fn write_managed_markdown(path: &Path, block: &str) -> io::Result<InstalledFile> {
    let existing = fs::read_to_string(path).ok();
    let next = match existing.as_deref() {
        Some(content) => replace_managed_block(content, block),
        None => format!("{block}\n"),
    };
    write_file_if_changed(path, &next)
}

fn replace_managed_block(content: &str, block: &str) -> String {
    let Some(start) = content.find(BEGIN) else {
        return append_block(content, block);
    };
    let Some(end_start) = content[start..].find(END).map(|offset| start + offset) else {
        return append_block(content, block);
    };
    let end = end_start + END.len();
    let mut out = String::new();
    out.push_str(&content[..start]);
    out.push_str(block);
    out.push_str(&content[end..]);
    out
}

fn append_block(content: &str, block: &str) -> String {
    let mut out = content.to_string();
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push('\n');
    out.push_str(block);
    out.push('\n');
    out
}

fn write_file_if_changed(path: &Path, content: &str) -> io::Result<InstalledFile> {
    let existed = path.exists();
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return Ok(InstalledFile {
            path: path.to_path_buf(),
            action: InstallAction::Unchanged,
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(InstalledFile {
        path: path.to_path_buf(),
        action: if existed {
            InstallAction::Updated
        } else {
            InstallAction::Created
        },
    })
}

fn agent_block() -> String {
    format!(
        r#"{BEGIN}
## flog Inspect

When this project needs Flutter log, network, WebSocket, SSE, or screenshot inspection, use `flog ai` instead of asking the user to copy from the TUI.

Use `.flog/skills/flog-inspect/SKILL.md` for the full workflow. Short version:

- Start with `flog ai snapshot --last 20 --net-last 20`.
- For visual/UI issues, add `--screenshot` or run `flog ai screenshot --device <device-id>`.
- Narrow before fetching large data: `flog ai logs --level error --last 20`, `flog ai net --failed --last 20`, or `flog ai net --method GET --url <part> --last 20`.
- Fetch details only after choosing an id: `flog ai get net#42 --detail`.
- Reproduce requests with `flog ai curl net#42`.
- Do not use `--no-redact` unless the user explicitly approves exposing secrets.
{END}"#
    )
}

fn claude_block() -> String {
    format!(
        r#"{BEGIN}
@AGENTS.md
{END}"#
    )
}

fn cursor_rule() -> String {
    r#"---
description: Use flog ai to inspect Flutter app logs, network traffic, current screen screenshots, SSE, or WebSocket debugging context.
alwaysApply: false
---

When the user asks to inspect Flutter app state, logs, HTTP traffic, SSE/WebSocket streams, or UI/screenshots, use the shared project guidance in `AGENTS.md` and the full workflow in `.flog/skills/flog-inspect/SKILL.md`.

Start small with `flog ai snapshot --last 20 --net-last 20`, then narrow with `flog ai logs`, `flog ai net`, `flog ai get --detail`, `flog ai curl`, or `flog ai screenshot` only as needed. Do not use `--no-redact` unless the user explicitly approves exposing secrets.
"#
    .to_string()
}

fn action_label(action: InstallAction) -> &'static str {
    match action {
        InstallAction::Created => "created",
        InstallAction::Updated => "updated",
        InstallAction::Unchanged => "unchanged",
    }
}

#[cfg(test)]
#[path = "install_skill_tests.rs"]
mod tests;
