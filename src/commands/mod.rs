//! Non-TUI maintenance commands.

use std::io;

use crate::cli::Command;

mod cli_ui;
pub(crate) mod ai;
mod devices;
mod doctor;
mod uninstall;
mod update;

pub async fn run(command: Command) -> io::Result<()> {
    match command {
        Command::Update => update::run().await,
        Command::Uninstall => uninstall::run().await,
        Command::Doctor => doctor::run().await,
        Command::Devices => devices::run().await,
        Command::Ai(command) => ai::run(command).await,
    }
}
