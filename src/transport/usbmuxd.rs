//! usbmuxd protocol client for iOS real device connectivity (macOS).

#[cfg(target_os = "macos")]
mod imp {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    const USBMUXD_SOCKET: &str = "/var/run/usbmuxd";
    const HEADER_SIZE: usize = 16;

    #[derive(Debug, Clone)]
    pub struct UsbDevice {
        pub device_id: u32,
        pub serial_number: String,
    }

    pub async fn list_devices() -> Result<Vec<UsbDevice>, Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = UnixStream::connect(USBMUXD_SOCKET).await?;

        let request = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "ListDevices".into());
            d.insert("ClientVersionString".into(), "flog".into());
            d.insert("ProgName".into(), "flog".into());
            d
        });
        send_plist(&mut stream, &request, 1).await?;

        let (_, response) = recv_plist(&mut stream).await?;

        let mut devices = Vec::new();
        if let Some(plist::Value::Array(list)) = response.as_dictionary().and_then(|d| d.get("DeviceList")) {
            for dev in list {
                if let Some(props) = dev.as_dictionary().and_then(|d| d.get("Properties")).and_then(|p| p.as_dictionary()) {
                    let device_id = props.get("DeviceID").and_then(|v| v.as_unsigned_integer()).unwrap_or(0) as u32;
                    let serial = props.get("SerialNumber").and_then(|v| v.as_string()).unwrap_or("").to_string();
                    if !serial.is_empty() {
                        devices.push(UsbDevice { device_id, serial_number: serial });
                    }
                }
            }
        }
        Ok(devices)
    }

    pub async fn connect_device(device_id: u32, port: u16) -> Result<UnixStream, Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = UnixStream::connect(USBMUXD_SOCKET).await?;

        let port_be = (port as u32).to_be();
        let request = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "Connect".into());
            d.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
            d.insert("PortNumber".into(), plist::Value::Integer(port_be.into()));
            d.insert("ClientVersionString".into(), "flog".into());
            d.insert("ProgName".into(), "flog".into());
            d
        });
        send_plist(&mut stream, &request, 2).await?;

        let (_, response) = recv_plist(&mut stream).await?;

        let result_code = response
            .as_dictionary()
            .and_then(|d| d.get("Number"))
            .and_then(|v| v.as_unsigned_integer())
            .unwrap_or(u64::MAX);

        if result_code != 0 {
            return Err(format!("usbmuxd Connect failed: code {}", result_code).into());
        }

        Ok(stream)
    }

    /// Query device name via lockdownd (port 62078) through usbmuxd tunnel.
    /// No external tools required — pure usbmuxd + lockdownd protocol.
    pub async fn query_device_name(device_id: u32) -> Option<String> {
        // Timeout the entire query — lockdownd may be slow if device is locked
        tokio::time::timeout(std::time::Duration::from_secs(3), query_device_name_inner(device_id)).await.ok()?
    }

    async fn query_device_name_inner(device_id: u32) -> Option<String> {
        let mut stream = UnixStream::connect(USBMUXD_SOCKET).await.ok()?;

        // Connect to lockdownd (port 62078) via usbmuxd
        // usbmuxd expects port as htons(port): u16 byte-swap, then zero-extended to u32
        let port_be = 62078u16.swap_bytes() as u32;
        let request = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "Connect".into());
            d.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
            d.insert("PortNumber".into(), plist::Value::Integer(port_be.into()));
            d.insert("ClientVersionString".into(), "flog".into());
            d.insert("ProgName".into(), "flog".into());
            d
        });
        send_plist(&mut stream, &request, 2).await.ok()?;

        let (_, response) = recv_plist(&mut stream).await.ok()?;
        let code = response.as_dictionary()?.get("Number")?.as_unsigned_integer()?;
        if code != 0 { return None; }

        // Query both DeviceName and MarketingName via lockdownd
        let device_name = lockdown_get_value(&mut stream, "DeviceName").await;
        let marketing_name = lockdown_get_value(&mut stream, "MarketingName").await;

        match (device_name, marketing_name) {
            (Some(dn), Some(mn)) => Some(format!("{} ({})", dn, mn)),
            (Some(dn), None) => Some(dn),
            (None, Some(mn)) => Some(mn),
            (None, None) => None,
        }
    }

    /// Query a single value from lockdownd.
    /// lockdownd uses big-endian 4-byte length prefix (different from usbmuxd).
    async fn lockdown_get_value(stream: &mut UnixStream, key: &str) -> Option<String> {
        let req = plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("Request".into(), "GetValue".into());
            d.insert("Key".into(), key.into());
            d
        });
        let mut body = Vec::new();
        req.to_writer_xml(&mut body).ok()?;
        stream.write_all(&(body.len() as u32).to_be_bytes()).await.ok()?;
        stream.write_all(&body).await.ok()?;
        stream.flush().await.ok()?;

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.ok()?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        let mut resp_body = vec![0u8; resp_len];
        stream.read_exact(&mut resp_body).await.ok()?;
        let value = plist::Value::from_reader(std::io::Cursor::new(resp_body)).ok()?;
        value.as_dictionary()?.get("Value")?.as_string().map(|s| s.to_string())
    }

    async fn send_plist(stream: &mut UnixStream, value: &plist::Value, tag: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let mut body = Vec::new();
        value.to_writer_xml(&mut body)?;

        let length = (HEADER_SIZE + body.len()) as u32;
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.extend_from_slice(&length.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes()); // version
        header.extend_from_slice(&8u32.to_le_bytes()); // type = plist
        header.extend_from_slice(&tag.to_le_bytes());

        stream.write_all(&header).await?;
        stream.write_all(&body).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn recv_plist(stream: &mut UnixStream) -> Result<(u32, plist::Value), Box<dyn std::error::Error + Send + Sync>> {
        let mut header = [0u8; HEADER_SIZE];
        stream.read_exact(&mut header).await?;

        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let tag = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);

        let body_len = length - HEADER_SIZE;
        let mut body = vec![0u8; body_len];
        stream.read_exact(&mut body).await?;

        let value = plist::Value::from_reader(std::io::Cursor::new(body))?;
        Ok((tag, value))
    }
}

// Re-export for macOS
#[cfg(target_os = "macos")]
pub use imp::*;

// Stub for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub mod imp {
    #[derive(Debug, Clone)]
    pub struct UsbDevice {
        pub device_id: u32,
        pub serial_number: String,
    }

    pub async fn list_devices() -> Result<Vec<UsbDevice>, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Vec::new())
    }

    pub async fn connect_device(_device_id: u32, _port: u16) -> Result<tokio::net::TcpStream, Box<dyn std::error::Error + Send + Sync>> {
        Err("usbmuxd is only available on macOS".into())
    }
}

#[cfg(not(target_os = "macos"))]
pub use imp::*;
