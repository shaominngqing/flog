use super::*;

#[test]
fn flutter_screenshot_command_uses_device_and_output() {
    let command = flutter_screenshot_command("emulator-5554", "/tmp/out.png");
    assert_eq!(command.program, "flutter");
    assert_eq!(
        command.args,
        vec!["screenshot", "-d", "emulator-5554", "-o", "/tmp/out.png"]
    );
}

#[test]
fn default_screenshot_path_is_png_under_temp_dir() {
    let path = default_screenshot_path("emulator-5554");
    assert!(path.to_string_lossy().contains("flog-ai"));
    assert!(path.to_string_lossy().ends_with(".png"));
}

#[test]
fn screenshot_failure_output_classifies_unsupported_without_stdout_leak() {
    let result = failure_with_output(
        "macos",
        b"Screenshot not supported for macOS.\n",
        b"Must have a connected device for screenshot type device\n",
    );

    assert!(!result.ok);
    let error = result.error.unwrap();
    assert!(matches!(error.code, AiErrorCode::ScreenshotUnsupported));
    assert!(error
        .message
        .contains("Screenshot not supported for macOS."));
    assert!(error.message.contains("Must have a connected device"));
}

#[test]
fn command_output_text_joins_stdout_and_stderr() {
    let output = command_output_text(b"stdout line\n", b"stderr line\n");
    assert_eq!(output, "stdout line stderr line");
}
