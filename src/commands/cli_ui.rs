//! Shared terminal output helpers for maintenance commands.

use std::io::{self, IsTerminal, Write};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

const MAUVE: &str = "38;5;183";
const BLUE: &str = "38;5;117";
const GREEN: &str = "38;5;120";
const YELLOW: &str = "38;5;229";
const RED: &str = "38;5;203";
const DIM: &str = "2";

pub struct CliUi {
    color: bool,
    dynamic: bool,
}

impl CliUi {
    pub fn new() -> Self {
        let dynamic = dynamic_output_enabled();
        Self {
            color: dynamic,
            dynamic,
        }
    }

    pub fn title(&self, text: &str) {
        println!("{} {}", self.paint("◆", MAUVE, true), text);
        println!();
    }

    pub fn section(&self, text: &str) {
        println!("  {} {}", self.paint("▸", BLUE, true), text);
    }

    pub fn ok(&self, label: &str, value: &str) {
        println!(
            "{}",
            status_line(&self.paint("✓", GREEN, true), label, value)
        );
    }

    pub fn warn(&self, label: &str, value: &str) {
        println!(
            "{}",
            status_line(&self.paint("!", YELLOW, true), label, value)
        );
    }

    pub fn fail(&self, label: &str, value: &str) {
        println!("{}", status_line(&self.paint("✕", RED, true), label, value));
    }

    pub fn empty(&self, value: &str) {
        println!("    {} {}", self.paint("-", DIM, false), value);
    }

    pub fn progress(&self, current: u64, total: u64) {
        let bar = progress_bar(current, total, 28, self.color);
        let pct = if total > 0 {
            current.min(total).saturating_mul(100) / total
        } else {
            0
        };
        if self.dynamic {
            print!(
                "\r    {}  {}/{}MB  {:>3}%",
                bar,
                mb(current),
                mb(total),
                pct
            );
            let _ = io::stdout().flush();
        } else {
            println!("    {}  {}/{}MB  {:>3}%", bar, mb(current), mb(total), pct);
        }
    }

    pub fn finish_progress(&self) {
        if self.dynamic {
            println!();
        }
    }

    pub fn spinner(&self, label: &str) -> Spinner {
        if self.dynamic {
            let active = Arc::new(AtomicBool::new(true));
            let active_for_thread = Arc::clone(&active);
            let label = label.to_string();
            let handle = std::thread::spawn(move || {
                let mut elapsed_ms = 0u64;
                while active_for_thread.load(Ordering::Relaxed) {
                    print!("\r    {} {}", spinner_frame(elapsed_ms), label);
                    let _ = io::stdout().flush();
                    std::thread::sleep(Duration::from_millis(120));
                    elapsed_ms = elapsed_ms.wrapping_add(120);
                }
            });
            Spinner {
                active: true,
                stop: Some(active),
                handle: Some(handle),
            }
        } else {
            println!("    - {}", label);
            Spinner {
                active: false,
                stop: None,
                handle: None,
            }
        }
    }

    fn paint(&self, s: &str, color: &str, bold: bool) -> String {
        if !self.color {
            return s.to_string();
        }
        let weight = if bold { "1;" } else { "" };
        format!("\x1b[{}{}m{}\x1b[0m", weight, color, s)
    }
}

pub struct Spinner {
    active: bool,
    stop: Option<Arc<AtomicBool>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn finish(mut self) {
        if !self.active {
            return;
        }
        if let Some(stop) = self.stop.take() {
            stop.store(false, Ordering::Relaxed);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        print!("\r\x1b[2K");
        let _ = io::stdout().flush();
    }
}

pub(crate) fn spinner_frame(elapsed_ms: u64) -> &'static str {
    let frames = ["◐", "◓", "◑", "◒"];
    frames[((elapsed_ms / 120) as usize) % frames.len()]
}

pub(crate) fn progress_bar(current: u64, total: u64, width: usize, color: bool) -> String {
    let filled = if total > 0 {
        ((current.min(total) as f64 / total as f64) * width as f64).round() as usize
    } else {
        0
    };
    let empty = width.saturating_sub(filled);
    if !color {
        return format!("{}{}", "━".repeat(filled), "░".repeat(empty));
    }
    format!(
        "\x1b[38;5;39m{}\x1b[2m{}\x1b[0m",
        "━".repeat(filled),
        "━".repeat(empty)
    )
}

pub(crate) fn status_line_plain(mark: &str, label: &str, value: &str) -> String {
    format!("    {} {:<8} {}", mark, label, value)
}

fn status_line(mark: &str, label: &str, value: &str) -> String {
    status_line_plain(mark, label, value)
}

fn mb(bytes: u64) -> String {
    format!("{:.1}", bytes as f64 / 1_048_576.0)
}

fn dynamic_output_enabled() -> bool {
    io::stdout().is_terminal()
        && std::env::var_os("NO_COLOR").is_none()
        && std::env::var_os("CI").is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_bar_renders_expected_width() {
        assert_eq!(progress_bar(50, 100, 10, false), "━━━━━░░░░░");
    }

    #[test]
    fn progress_bar_handles_zero_total() {
        assert_eq!(progress_bar(10, 0, 6, false), "░░░░░░");
    }

    #[test]
    fn status_line_aligns_label() {
        assert_eq!(
            status_line_plain("✓", "Version", "v0.5.2"),
            "    ✓ Version  v0.5.2"
        );
    }

    #[test]
    fn spinner_frame_cycles_by_elapsed_millis() {
        assert_eq!(spinner_frame(0), "◐");
        assert_eq!(spinner_frame(120), "◓");
        assert_eq!(spinner_frame(240), "◑");
        assert_eq!(spinner_frame(360), "◒");
        assert_eq!(spinner_frame(480), "◐");
    }
}
