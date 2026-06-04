use std::fs;

fn install_script() -> String {
    fs::read_to_string("install.sh").expect("install.sh should be readable")
}

#[test]
fn installer_uses_checked_in_release_version() {
    let script = install_script();

    assert!(
        script.contains("FLOG_VERSION=\"0.6.0\""),
        "installer should pin the published release version"
    );
    assert!(
        !script.contains("api.github.com/repos/${REPO}/releases/latest"),
        "installer should not depend on the GitHub latest-release API"
    );
}

#[test]
fn installer_downloads_with_retries_and_visible_progress() {
    let script = install_script();

    assert!(
        script.contains("--retry"),
        "installer downloads should retry transient network failures"
    );
    assert!(
        script.contains("--progress-bar"),
        "fallback download should show visible progress"
    );
}

#[test]
fn installer_prefers_current_flog_path_for_updates() {
    let script = install_script();

    assert!(script.contains("ACTIVE_FLOG=$(command -v flog"));
    assert!(script.contains("INSTALL_DIR=$(dirname \"$ACTIVE_FLOG\")"));
    assert!(
        script.contains("Your shell currently resolves flog to:"),
        "installer should warn when PATH resolves a different flog binary"
    );
}
