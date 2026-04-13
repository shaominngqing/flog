//! Test: find DDS proxy port and connect to it.
//!
//! Run with: cargo test --test ws_connect_test2 -- --nocapture

use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

/// Find the DDS listening ports by inspecting the DDS process with lsof.
#[tokio::test]
async fn test_find_dds_ports() {
    // Step 1: Find DDS PID from ps aux
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    let mut dds_pid: Option<String> = None;
    let mut underlying_port: Option<String> = None;

    for line in text.lines() {
        if line.contains("development-service") && line.contains("--vm-service-uri=") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 1 {
                dds_pid = Some(parts[1].to_string());
            }
            if let Some(idx) = line.find("--vm-service-uri=http://127.0.0.1:") {
                let rest = &line[idx + 34..];
                let end = rest.find('/').unwrap_or(rest.len());
                underlying_port = Some(rest[..end].to_string());
            }
        }
    }

    let dds_pid = match dds_pid {
        Some(p) => p,
        None => {
            println!("No DDS process found");
            return;
        }
    };

    println!("DDS PID: {}", dds_pid);
    println!("Underlying VM port: {:?}", underlying_port);

    // Step 2: Use lsof to find DDS's own listening ports (NOT -p flag which lists all)
    // Instead, grep for the specific PID in the output
    let lsof = tokio::process::Command::new("lsof")
        .args(["-iTCP", "-P", "-n", "-sTCP:LISTEN"])
        .output()
        .await
        .unwrap();

    let lsof_text = String::from_utf8_lossy(&lsof.stdout);
    let underlying = underlying_port.as_deref().unwrap_or("");

    println!("\nDDS (PID {}) LISTEN ports:", dds_pid);
    let mut dds_ports: Vec<u16> = Vec::new();
    for line in lsof_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() > 1 && parts[1] == dds_pid {
            println!("  {}", line);
            // Extract port
            if let Some(addr) = parts.get(8) {
                if let Some(port_str) = addr.rsplit(':').next() {
                    if port_str != underlying {
                        if let Ok(port) = port_str.parse::<u16>() {
                            dds_ports.push(port);
                        }
                    }
                }
            }
        }
    }

    println!(
        "\nDDS ports (excluding underlying {}): {:?}",
        underlying, dds_ports
    );

    // Step 3: Try HTTP GET on each DDS port to find the auth token
    for port in &dds_ports {
        println!("\n=== Trying port {} ===", port);
        if let Ok(Ok(mut stream)) = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)),
        )
        .await
        {
            let req = format!(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                port
            );
            let _ = stream.write_all(req.as_bytes()).await;
            let mut buf = vec![0u8; 8192];
            if let Ok(Ok(n)) =
                tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await
            {
                let resp = String::from_utf8_lossy(&buf[..n]);
                println!(
                    "HTTP response ({} bytes):\n{}",
                    n,
                    &resp[..resp.len().min(1000)]
                );
            }
        }
    }
}

/// Try connecting to the DDS proxy port with different auth token patterns.
#[tokio::test]
async fn test_connect_dds_ws() {
    // Find DDS PID and ports
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    let mut dds_pid: Option<String> = None;

    for line in text.lines() {
        if line.contains("development-service") && line.contains("--vm-service-uri=") {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 1 {
                dds_pid = Some(parts[1].to_string());
            }
        }
    }

    let dds_pid = match dds_pid {
        Some(p) => p,
        None => {
            println!("No DDS process found");
            return;
        }
    };

    // Find DDS listening ports
    let lsof = tokio::process::Command::new("lsof")
        .args(["-iTCP", "-P", "-n", "-sTCP:LISTEN"])
        .output()
        .await
        .unwrap();

    let lsof_text = String::from_utf8_lossy(&lsof.stdout);
    let mut dds_ports: Vec<u16> = Vec::new();
    for line in lsof_text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() > 1 && parts[1] == dds_pid {
            if let Some(addr) = parts.get(8) {
                if let Some(port_str) = addr.rsplit(':').next() {
                    if let Ok(port) = port_str.parse::<u16>() {
                        dds_ports.push(port);
                    }
                }
            }
        }
    }

    dds_ports.sort();
    dds_ports.dedup();
    println!("DDS ports: {:?}", dds_ports);

    // For each port, try to get the VM Service info via HTTP
    for port in &dds_ports {
        println!("\n=== Testing port {} ===", port);

        // Try 1: GET / to see if there's a redirect or info page
        if let Ok(Ok(mut stream)) = tokio::time::timeout(
            Duration::from_secs(2),
            tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)),
        )
        .await
        {
            let req = format!(
                "GET /getVM HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                port
            );
            let _ = stream.write_all(req.as_bytes()).await;
            let mut buf = vec![0u8; 8192];
            if let Ok(Ok(n)) =
                tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await
            {
                let resp = String::from_utf8_lossy(&buf[..n]);
                println!("GET /getVM response:\n{}", &resp[..resp.len().min(500)]);
            }
        }

        // Try 2: WebSocket without auth
        let ws_url = format!("ws://127.0.0.1:{}/ws", port);
        println!("\nTrying WS: {}", ws_url);
        match tokio::time::timeout(Duration::from_secs(3), connect_async(&ws_url)).await {
            Ok(Ok((ws, resp))) => {
                println!("Connected! Status: {}", resp.status());
                let (mut tx, mut rx) = ws.split();

                let get_vm = serde_json::json!({
                    "jsonrpc": "2.0", "method": "getVM", "params": {}, "id": "1"
                });
                tx.send(Message::Text(get_vm.to_string().into()))
                    .await
                    .unwrap();

                match tokio::time::timeout(Duration::from_secs(3), rx.next()).await {
                    Ok(Some(Ok(Message::Text(t)))) => {
                        println!("getVM response: {}", &t[..t.len().min(300)]);
                    }
                    other => println!("Unexpected: {:?}", other),
                }
                return; // Found it!
            }
            Ok(Err(e)) => {
                println!("WS failed: {}", e);
            }
            Err(_) => {
                println!("WS timed out");
            }
        }
    }
}

/// The CORRECT approach: read the DDS stdout to find the actual service URL.
/// DDS writes its URL to stdout which flutter_tools reads.
/// But since we can't read DDS's stdout, we use the Dart Tooling Daemon
/// on another port to query for the VM Service URI.
///
/// Alternative: connect to the devtools server's WebSocket to get the URI.
/// The devtools process uses: --dtd-uri ws://127.0.0.1:PORT/TOKEN=
/// This is the Dart Tooling Daemon URI. We can use it!
#[tokio::test]
async fn test_dart_tooling_daemon() {
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    // Find the DTD URI from the devtools process
    let mut dtd_uri: Option<String> = None;
    for line in text.lines() {
        if line.contains("--dtd-uri") {
            if let Some(idx) = line.find("--dtd-uri ") {
                let rest = &line[idx + 10..];
                let end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
                dtd_uri = Some(rest[..end].to_string());
            }
        }
    }

    println!("DTD URI from devtools: {:?}", dtd_uri);

    if let Some(uri) = dtd_uri {
        println!("\nConnecting to DTD: {}", uri);
        match tokio::time::timeout(Duration::from_secs(3), connect_async(&uri)).await {
            Ok(Ok((ws, _))) => {
                println!("Connected to DTD!");
                let (mut tx, mut rx) = ws.split();

                // DTD uses JSON-RPC. Try to get VM Service URI.
                // The DTD protocol has a "getRegisteredStreamServices" or similar.
                // Let's try some known methods.
                for (id, method) in [("1", "streamListen"), ("2", "getAvailableStreams")] {
                    let msg = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": method,
                        "params": {},
                        "id": id,
                    });
                    let _ = tx.send(Message::Text(msg.to_string().into())).await;
                }

                let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
                loop {
                    match tokio::time::timeout_at(deadline, rx.next()).await {
                        Ok(Some(Ok(Message::Text(t)))) => {
                            println!("DTD msg: {}", &t[..t.len().min(500)]);
                        }
                        Ok(None) | Err(_) => break,
                        _ => continue,
                    }
                }
            }
            Ok(Err(e)) => println!("DTD connect failed: {}", e),
            Err(_) => println!("DTD connect timed out"),
        }
    }
}

/// The simplest correct approach:
/// flutter_tools prints "The Dart VM service is listening on http://127.0.0.1:PORT/TOKEN=/"
/// to its stdout. DDS's stdout also contains this.
///
/// BUT the DDS process stdout goes to flutter_tools via pipes (fd 0/1).
/// We can read it from flutter_tools' stdout — but that goes to the terminal.
///
/// Let's try yet another approach: check if there's a .flutter-devtools or
/// .dart_tool file that stores the DDS URL.
#[tokio::test]
async fn test_check_flutter_devtools_files() {
    // Check common places where Flutter stores runtime info
    let home = std::env::var("HOME").unwrap_or_default();
    let paths = [
        format!("{}/.flutter-devtools", home),
        format!("{}/.dart_tool", home),
        format!("{}/FlutterProject/aura-lang-flutter/.dart_tool", home),
    ];

    for path in &paths {
        println!("\nChecking: {}", path);
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                println!("  {}", name);
                // Read small files to look for URLs
                if name.ends_with(".json") || name.ends_with(".txt") || name.ends_with(".log") {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if content.len() < 5000
                            && (content.contains("127.0.0.1") || content.contains("ws://"))
                        {
                            println!("    Content: {}", &content[..content.len().min(200)]);
                        }
                    }
                }
            }
        } else {
            println!("  (not found)");
        }
    }
}

/// The REAL correct approach: connect to underlying VM Service HTTP to see what happens.
/// The 302 redirect might contain the DDS URL!
#[tokio::test]
async fn test_follow_302_redirect() {
    let output = tokio::process::Command::new("ps")
        .args(["aux"])
        .output()
        .await
        .unwrap();
    let text = String::from_utf8_lossy(&output.stdout);

    let mut underlying_url: Option<String> = None;
    for line in text.lines() {
        if let Some(idx) = line.find("--vm-service-uri=") {
            let rest = &line[idx + 17..];
            let url_end = rest.find(|c: char| c.is_whitespace()).unwrap_or(rest.len());
            underlying_url = Some(rest[..url_end].trim_end_matches('/').to_string());
        }
    }

    let url = match underlying_url {
        Some(u) => u,
        None => {
            println!("No URL found");
            return;
        }
    };

    println!("Underlying URL: {}", url);

    // Try HTTP GET and look at the redirect
    let host_port = url
        .strip_prefix("http://")
        .and_then(|s| s.split('/').next())
        .unwrap_or("127.0.0.1");
    let path = url
        .strip_prefix(&format!("http://{}", host_port))
        .unwrap_or("/");

    if let Ok(Ok(mut stream)) = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(host_port),
    )
    .await
    {
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            path, host_port
        );
        let _ = stream.write_all(req.as_bytes()).await;
        let mut buf = vec![0u8; 8192];
        if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await
        {
            let resp = String::from_utf8_lossy(&buf[..n]);
            println!("\nHTTP GET {} response:\n{}", path, resp);
        }
    }

    // Also try the /ws path via HTTP (to see what WebSocket upgrade error looks like)
    let ws_path = format!("{}/ws", path.trim_end_matches('/'));
    println!("\n--- Also trying GET {}ws ---", path);
    if let Ok(Ok(mut stream)) = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::net::TcpStream::connect(host_port),
    )
    .await
    {
        let req = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
            ws_path, host_port
        );
        let _ = stream.write_all(req.as_bytes()).await;
        let mut buf = vec![0u8; 8192];
        if let Ok(Ok(n)) = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf)).await
        {
            let resp = String::from_utf8_lossy(&buf[..n]);
            println!("HTTP GET {} response:\n{}", ws_path, resp);
        }
    }
}
