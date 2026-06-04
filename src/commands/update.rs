use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use futures_util::StreamExt;
use serde::Deserialize;

use super::cli_ui::CliUi;

const REPO: &str = "shaominngqing/flog";

pub async fn run() -> io::Result<()> {
    let ui = CliUi::new();
    ui.title("flog update");
    ui.section("Checking release");

    let current = env!("CARGO_PKG_VERSION");
    ui.ok("Current", &format!("v{}", current));

    let release = match latest_release().await {
        Ok(release) => release,
        Err(e) => {
            ui.fail("GitHub", &e.to_string());
            return Ok(());
        }
    };
    let latest = version_from_tag(&release.tag_name);
    ui.ok("Latest", &format!("v{}", latest));

    if latest == current {
        ui.empty("Already up to date");
        return Ok(());
    }

    let exe = std::env::current_exe()?;
    if is_development_binary(&exe) {
        ui.warn("Binary", "development build");
        ui.empty("Run the install script or cargo install for installed binaries");
        return Ok(());
    }

    let platform = match platform() {
        Ok(platform) => platform,
        Err(e) => {
            ui.fail("Platform", &e.to_string());
            return Ok(());
        }
    };
    ui.ok(
        "Platform",
        &format!("{} {}", platform.os_label, platform.arch),
    );

    let asset = asset_name(&platform.os, &platform.arch);
    let Some(url) = release.asset_url(&asset) else {
        ui.fail("Asset", &format!("missing {}", asset));
        return Ok(());
    };
    ui.ok("Binary", &exe.display().to_string());

    print!("\n  Update {}? [y/N] ", exe.display());
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    if !confirm_answer(&answer) {
        ui.empty("Cancelled");
        return Ok(());
    }

    let tmp_dir = std::env::temp_dir().join(format!("flog-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir)?;
    let archive = tmp_dir.join(&asset);

    ui.section("Downloading");
    if let Err(e) = download(&url, &archive, &ui).await {
        ui.fail("Download", &e.to_string());
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }

    ui.section("Installing");
    let extracted = match extract_archive(&archive, &tmp_dir) {
        Ok(path) => path,
        Err(e) => {
            ui.fail("Extract", &e.to_string());
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Ok(());
        }
    };
    if let Err(e) = verify_binary(&extracted) {
        ui.fail("Verify", &e.to_string());
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }
    if let Err(e) = replace_current_exe(&exe, &extracted) {
        ui.fail("Install", &e.to_string());
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Ok(());
    }

    ui.ok("Updated", &format!("v{}", latest));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

#[derive(Debug, Deserialize)]
pub(crate) struct Release {
    pub(crate) tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

impl Release {
    fn asset_url(&self, name: &str) -> Option<String> {
        self.assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
    }
}

struct Platform {
    os: String,
    os_label: String,
    arch: String,
}

pub(crate) async fn latest_release() -> Result<Release, Box<dyn std::error::Error + Send + Sync>> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", REPO);
    let body = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "flog")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(serde_json::from_str(&body)?)
}

pub(crate) fn version_from_tag(tag: &str) -> String {
    tag.strip_prefix('v').unwrap_or(tag).to_string()
}

pub(crate) fn asset_name(os: &str, arch: &str) -> String {
    let ext = if os == "windows" { "zip" } else { "tar.gz" };
    format!("flog-{}-{}.{}", os, arch, ext)
}

pub(crate) fn confirm_answer(answer: &str) -> bool {
    matches!(answer.trim(), "y" | "Y" | "yes" | "YES")
}

fn platform() -> Result<Platform, String> {
    let os = match std::env::consts::OS {
        "macos" => ("macos", "macOS"),
        "linux" => ("linux", "Linux"),
        "windows" => ("windows", "Windows"),
        other => return Err(format!("unsupported OS: {}", other)),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        other => return Err(format!("unsupported architecture: {}", other)),
    };
    Ok(Platform {
        os: os.0.to_string(),
        os_label: os.1.to_string(),
        arch: arch.to_string(),
    })
}

async fn download(
    url: &str,
    path: &Path,
    ui: &CliUi,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let response = reqwest::Client::new()
        .get(url)
        .header("User-Agent", "flog")
        .send()
        .await?
        .error_for_status()?;
    let total = response.content_length().unwrap_or(0);
    let mut stream = response.bytes_stream();
    let mut file = std::fs::File::create(path)?;
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        ui.progress(downloaded, total);
    }
    file.flush()?;
    ui.finish_progress();
    Ok(())
}

fn extract_archive(
    archive: &Path,
    tmp_dir: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error + Send + Sync>> {
    if archive.extension().and_then(|s| s.to_str()) == Some("zip") {
        let status = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "Expand-Archive",
                "-Force",
                archive.to_string_lossy().as_ref(),
                tmp_dir.to_string_lossy().as_ref(),
            ])
            .status()?;
        if !status.success() {
            return Err("zip extraction failed".into());
        }
    } else {
        let status = Command::new("tar")
            .arg("xzf")
            .arg(archive)
            .arg("-C")
            .arg(tmp_dir)
            .status()?;
        if !status.success() {
            return Err("tar extraction failed".into());
        }
    }

    let binary = tmp_dir.join(if cfg!(windows) { "flog.exe" } else { "flog" });
    if binary.exists() {
        Ok(binary)
    } else {
        Err("archive did not contain flog binary".into())
    }
}

fn verify_binary(binary: &Path) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let output = Command::new(binary).arg("--version").output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err("downloaded binary failed --version".into())
    }
}

fn replace_current_exe(current: &Path, replacement: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        let _ = (current, replacement);
        return Err(io::Error::other(
            "self-update is not supported on Windows yet; rerun install script",
        ));
    }

    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;

        let backup = current.with_extension("old");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(current, &backup)?;
        let result = (|| -> io::Result<()> {
            std::fs::copy(replacement, current)?;
            let mut perms = std::fs::metadata(current)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(current, perms)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(current);
            let _ = std::fs::rename(&backup, current);
        } else {
            let _ = std::fs::remove_file(&backup);
        }
        result
    }
}

fn is_development_binary(path: &Path) -> bool {
    let text = path.to_string_lossy();
    text.contains("/target/debug/") || text.contains("/target/release/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_name_for_macos_aarch64() {
        assert_eq!(asset_name("macos", "aarch64"), "flog-macos-aarch64.tar.gz");
    }

    #[test]
    fn asset_name_for_windows_x86_64() {
        assert_eq!(asset_name("windows", "x86_64"), "flog-windows-x86_64.zip");
    }

    #[test]
    fn normalize_latest_tag_strips_v() {
        assert_eq!(version_from_tag("v0.5.3"), "0.5.3");
    }

    #[test]
    fn confirm_answer_accepts_only_explicit_yes() {
        assert!(confirm_answer("y"));
        assert!(confirm_answer("YES"));
        assert!(!confirm_answer(""));
        assert!(!confirm_answer("n"));
    }
}
