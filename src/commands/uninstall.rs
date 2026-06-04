use std::io::{self, Write};
use std::path::PathBuf;

use super::cli_ui::CliUi;

pub struct UninstallPlan {
    pub binary: PathBuf,
    pub config_dir: Option<PathBuf>,
}

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog uninstall");

    let plan = uninstall_plan(std::env::current_exe()?, dirs::config_dir());
    ui.section("Remove");
    ui.ok("Binary", &plan.binary.display().to_string());
    if let Some(config_dir) = &plan.config_dir {
        ui.ok("Data", &config_dir.display().to_string());
    }
    ui.empty(uninstall_note());

    print!("\n  Continue? [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !matches!(answer.trim(), "y" | "Y" | "yes" | "YES") {
        ui.empty("Cancelled");
        return Ok(());
    }

    if let Some(config_dir) = &plan.config_dir {
        if config_dir.exists() {
            std::fs::remove_dir_all(config_dir)?;
        }
    }
    std::fs::remove_file(&plan.binary)?;
    ui.ok("Removed", "flog");
    Ok(())
}

pub fn uninstall_plan(binary: PathBuf, config_base: Option<PathBuf>) -> UninstallPlan {
    UninstallPlan {
        binary,
        config_dir: config_base.map(|path| path.join("flog")),
    }
}

pub(crate) fn uninstall_note() -> &'static str {
    "User exports like flog_*.log are not removed"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn uninstall_plan_contains_binary_and_config_dir() {
        let plan = uninstall_plan(
            PathBuf::from("/tmp/flog"),
            Some(PathBuf::from("/tmp/config")),
        );
        assert_eq!(plan.binary, PathBuf::from("/tmp/flog"));
        assert_eq!(plan.config_dir, Some(PathBuf::from("/tmp/config/flog")));
    }

    #[test]
    fn uninstall_note_mentions_exports_are_kept() {
        assert!(uninstall_note().contains("flog_*.log"));
        assert!(uninstall_note().contains("not removed"));
    }
}
