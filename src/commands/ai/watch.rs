use std::io;

pub async fn run_watch(duration: std::time::Duration) -> io::Result<()> {
    tokio::time::sleep(duration).await;
    Ok(())
}
