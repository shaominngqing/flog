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
