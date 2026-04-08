//! Stdin pipe input source.
//!
//! Reads lines from stdin, useful for: `flutter run 2>&1 | flog --stdin`

use tokio::io::{AsyncBufReadExt, BufReader};

use super::SourceEvent;

pub struct StdinSource {
    reader: tokio::io::Lines<BufReader<tokio::io::Stdin>>,
}

impl Default for StdinSource {
    fn default() -> Self {
        Self::new()
    }
}

impl StdinSource {
    pub fn new() -> Self {
        let reader = BufReader::new(tokio::io::stdin()).lines();
        Self { reader }
    }
}

impl StdinSource {
    pub async fn next_event(&mut self) -> Option<SourceEvent> {
        loop {
            match self.reader.next_line().await {
                Ok(Some(line)) => return Some(SourceEvent::RawLine(line)),
                Ok(None) => return None,
                Err(_) => continue,
            }
        }
    }

    pub fn name(&self) -> &str {
        "stdin"
    }
}
