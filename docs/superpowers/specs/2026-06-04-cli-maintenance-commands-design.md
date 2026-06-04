# CLI Maintenance Commands Design

## Goal

Add four basic maintenance commands to `flog`:

```bash
flog update
flog uninstall
flog doctor
flog devices
```

`flog` with no subcommand keeps the current behavior and starts the TUI.

## Scope

This feature intentionally stays small. There are no flags, no JSON mode,
no dry-run mode, no version picker, and no shell-completion work.

The commands should feel polished: colored status markers, compact sections,
loading spinners for slow checks, and a download progress bar for update.
All dynamic output must degrade to plain text when stdout is not a TTY, CI is
set, or `NO_COLOR` is set.

## Command Behavior

### `flog update`

`flog update` checks the latest GitHub release for
`shaominngqing/flog`, resolves the current platform asset using the same
asset naming as `install.sh`, downloads it, extracts `flog`, verifies the
new binary by running `flog --version`, and replaces the current executable.

If any step fails, the existing binary remains in place and the command
prints the failing step plus the next useful action.

### `flog uninstall`

`flog uninstall` shows the current binary path and the config directory that
will be removed. After a confirmation prompt, it removes:

- the current `flog` executable
- `dirs::config_dir()/flog`

User-created exports such as `flog_*.log` are not deleted. They are user
data because the user explicitly exported them into their working directory.

### `flog doctor`

`flog doctor` prints a compact diagnostic report:

- current version
- current executable path
- GitHub latest-release reachability
- `adb` availability
- macOS usbmuxd socket availability
- port status for `9753..9762`

The GitHub check is the network check for upgrade readiness.

### `flog devices`

`flog devices` scans the default `9753..9762` port range and reports
discovered devices and any connected `flog_dart` apps. It should use the
existing transport/discovery model where possible, and should avoid entering
the TUI or long-lived reconnect loops.

## Output Style

Use a small internal CLI UI helper rather than scattering ANSI escapes:

```text
◆ flog update

  ▸ Checking release
    ✓ Current  v0.5.2
    ✓ Latest   v0.5.3

  ▸ Downloading
    ━━━━━━━━━━━━━━━━━━━━━░░░░░░░░  4.1/5.2MB  79%

  ✓ Updated to v0.5.3
```

Symbols:

- `◆` command title
- `▸` section title
- `✓` success
- `!` warning
- `✕` failure
- `-` empty state

Colors should follow the existing Catppuccin feel: mauve/title, blue/info,
green/success, yellow/warning, red/error, dim/secondary.

## Architecture

Keep command code out of `main.rs` and out of the TUI/app layer.

```text
src/commands/
  mod.rs
  cli_ui.rs
  update.rs
  uninstall.rs
  doctor.rs
  devices.rs
```

`src/cli.rs` owns clap parsing. `src/main.rs` dispatches to a command when
one is present; otherwise it follows the existing TUI startup path.

The command layer may depend on `input` and `transport`, but `domain`,
`parser`, `input`, `transport`, and `app` must not depend on UI/TUI code.

## Testing

Use focused Rust tests for pure boundaries:

- clap parses each subcommand
- asset names map correctly by OS/arch
- CLI color disables outside TTY/when requested
- uninstall deletion plan includes binary and config directory
- doctor port-status formatting stays stable
- devices output formatting stays stable

Commands that shell out, hit GitHub, or require physical devices are tested
through pure helpers plus manual verification.
