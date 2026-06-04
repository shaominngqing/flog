//! AI-oriented headless inspection commands.

pub mod args;

use std::io;

use crate::cli::AiCommand;

pub async fn run(command: AiCommand) -> io::Result<()> {
    match command {
        AiCommand::Snapshot(_) => Ok(()),
        AiCommand::Watch(_) => Ok(()),
        AiCommand::Get(_) => Ok(()),
        AiCommand::Doctor(_) => Ok(()),
        AiCommand::Screenshot(_) => Ok(()),
    }
}
