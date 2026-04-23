//! usbmuxd protocol client for iOS real device connectivity (macOS).

#[cfg(target_os = "macos")]
mod imp {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    const USBMUXD_SOCKET: &str = "/var/run/usbmuxd";
    const HEADER_SIZE: usize = 16;

    pub async fn connect_device(
        device_id: u32,
        port: u16,
    ) -> Result<UnixStream, Box<dyn std::error::Error + Send + Sync>> {
        let mut stream = UnixStream::connect(USBMUXD_SOCKET).await?;

        let request = build_connect_request(device_id, port);
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
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            query_device_name_inner(device_id),
        )
        .await
        .ok()?
    }

    async fn query_device_name_inner(device_id: u32) -> Option<String> {
        let mut stream = UnixStream::connect(USBMUXD_SOCKET).await.ok()?;

        // Connect to lockdownd (port 62078) via usbmuxd.
        let request = build_connect_request(device_id, 62078);
        send_plist(&mut stream, &request, 2).await.ok()?;

        let (_, response) = recv_plist(&mut stream).await.ok()?;
        let code = response
            .as_dictionary()?
            .get("Number")?
            .as_unsigned_integer()?;
        if code != 0 {
            return None;
        }

        // Query both DeviceName and MarketingName via lockdownd
        let device_name = lockdown_get_value(&mut stream, "DeviceName").await;
        let marketing_name = lockdown_get_value(&mut stream, "MarketingName").await;

        match (device_name, marketing_name) {
            (Some(dn), Some(mn)) => Some(format!("{} ({})", mn, dn)),
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
        stream
            .write_all(&(body.len() as u32).to_be_bytes())
            .await
            .ok()?;
        stream.write_all(&body).await.ok()?;
        stream.flush().await.ok()?;

        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await.ok()?;
        let resp_len = u32::from_be_bytes(len_buf) as usize;
        let mut resp_body = vec![0u8; resp_len];
        stream.read_exact(&mut resp_body).await.ok()?;
        let value = plist::Value::from_reader(std::io::Cursor::new(resp_body)).ok()?;
        value
            .as_dictionary()?
            .get("Value")?
            .as_string()
            .map(|s| s.to_string())
    }

    async fn send_plist(
        stream: &mut UnixStream,
        value: &plist::Value,
        tag: u32,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (header, body) = encode_plist_frame(value, tag)?;
        stream.write_all(&header).await?;
        stream.write_all(&body).await?;
        stream.flush().await?;
        Ok(())
    }

    async fn recv_plist(
        stream: &mut UnixStream,
    ) -> Result<(u32, plist::Value), Box<dyn std::error::Error + Send + Sync>> {
        let mut header = [0u8; HEADER_SIZE];
        stream.read_exact(&mut header).await?;

        let (tag, body_len) = decode_plist_header(&header);

        let mut body = vec![0u8; body_len];
        stream.read_exact(&mut body).await?;

        let value = plist::Value::from_reader(std::io::Cursor::new(body))?;
        Ok((tag, value))
    }

    /// Build the (header, body) pair for a usbmuxd frame. Split off from
    /// send_plist so tests can assert the wire format without a UnixStream.
    pub(super) fn encode_plist_frame(
        value: &plist::Value,
        tag: u32,
    ) -> Result<(Vec<u8>, Vec<u8>), Box<dyn std::error::Error + Send + Sync>> {
        let mut body = Vec::new();
        value.to_writer_xml(&mut body)?;

        let length = (HEADER_SIZE + body.len()) as u32;
        let mut header = Vec::with_capacity(HEADER_SIZE);
        header.extend_from_slice(&length.to_le_bytes());
        header.extend_from_slice(&1u32.to_le_bytes()); // version
        header.extend_from_slice(&8u32.to_le_bytes()); // type = plist
        header.extend_from_slice(&tag.to_le_bytes());
        Ok((header, body))
    }

    /// Split off from recv_plist: returns (tag, body_len).
    pub(super) fn decode_plist_header(header: &[u8; HEADER_SIZE]) -> (u32, usize) {
        let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
        let tag = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        (tag, length.saturating_sub(HEADER_SIZE))
    }

    /// Port encoding for a usbmuxd Connect request: htons(port) zero-extended
    /// to u32. Tested independently so the value landing in PortNumber is
    /// unambiguous on both endianness spectra.
    pub(super) fn connect_port_field(port: u16) -> u32 {
        port.swap_bytes() as u32
    }

    /// Build the plist dictionary sent as the usbmuxd Connect request body.
    /// Pure — no I/O, no UnixStream required for tests.
    pub(super) fn build_connect_request(device_id: u32, port: u16) -> plist::Value {
        plist::Value::Dictionary({
            let mut d = plist::Dictionary::new();
            d.insert("MessageType".into(), "Connect".into());
            d.insert("DeviceID".into(), plist::Value::Integer(device_id.into()));
            d.insert(
                "PortNumber".into(),
                plist::Value::Integer(connect_port_field(port).into()),
            );
            d.insert("ClientVersionString".into(), "flog".into());
            d.insert("ProgName".into(), "flog".into());
            d
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn connect_port_field_byte_swaps_u16() {
            // 62078 = 0xF27E -> swap_bytes -> 0x7EF2 = 32498. Sent by lockdownd
            // pathway in query_device_name_inner; locking in the exact value
            // guards against an accidental sign/width change.
            assert_eq!(connect_port_field(62078), 0x7EF2);
            // 9753 = 0x2619 -> 0x1926 = 6438. flog_dart's default port.
            assert_eq!(connect_port_field(9753), 0x1926);
            assert_eq!(connect_port_field(0), 0);
            assert_eq!(connect_port_field(u16::MAX), 0x0000_FFFF);
            // General property: swap_bytes is its own inverse on u16.
            for p in [1u16, 80, 443, 8080, 32768] {
                let f = connect_port_field(p) as u16;
                assert_eq!(f.swap_bytes(), p);
            }
        }

        #[test]
        fn build_connect_request_contains_required_fields() {
            let v = build_connect_request(42, 9753);
            let d = v.as_dictionary().expect("dict");
            assert_eq!(d.get("MessageType").unwrap().as_string(), Some("Connect"));
            assert_eq!(
                d.get("DeviceID").unwrap().as_unsigned_integer(),
                Some(42u64)
            );
            assert_eq!(
                d.get("PortNumber").unwrap().as_unsigned_integer(),
                Some(connect_port_field(9753) as u64)
            );
            assert_eq!(
                d.get("ClientVersionString").unwrap().as_string(),
                Some("flog")
            );
            assert_eq!(d.get("ProgName").unwrap().as_string(), Some("flog"));
        }

        #[test]
        fn encode_frame_header_layout_is_usbmuxd_compliant() {
            let dict = plist::Value::Dictionary({
                let mut d = plist::Dictionary::new();
                d.insert("MessageType".into(), "Listen".into());
                d
            });
            let (header, body) = encode_plist_frame(&dict, 7).expect("encode");
            assert_eq!(header.len(), HEADER_SIZE);

            // Header word 0: total length including header.
            let length = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            assert_eq!(length as usize, HEADER_SIZE + body.len());

            // Word 1: version == 1.
            assert_eq!(
                u32::from_le_bytes([header[4], header[5], header[6], header[7]]),
                1
            );
            // Word 2: type == 8 (plist).
            assert_eq!(
                u32::from_le_bytes([header[8], header[9], header[10], header[11]]),
                8
            );
            // Word 3: tag passthrough.
            assert_eq!(
                u32::from_le_bytes([header[12], header[13], header[14], header[15]]),
                7
            );
        }

        #[test]
        fn decode_header_recovers_tag_and_body_len() {
            // Craft a header for a 100-byte frame (16 hdr + 84 body) with tag 9.
            let mut h = [0u8; HEADER_SIZE];
            h[0..4].copy_from_slice(&100u32.to_le_bytes());
            h[4..8].copy_from_slice(&1u32.to_le_bytes());
            h[8..12].copy_from_slice(&8u32.to_le_bytes());
            h[12..16].copy_from_slice(&9u32.to_le_bytes());

            let (tag, body_len) = decode_plist_header(&h);
            assert_eq!(tag, 9);
            assert_eq!(body_len, 84);
        }

        #[test]
        fn decode_header_handles_undersized_length_gracefully() {
            // A corrupt header reporting length < HEADER_SIZE would previously
            // underflow `length - HEADER_SIZE`. Use saturating sub to be safe.
            let mut h = [0u8; HEADER_SIZE];
            h[0..4].copy_from_slice(&4u32.to_le_bytes()); // < HEADER_SIZE
            h[12..16].copy_from_slice(&0u32.to_le_bytes());
            let (_tag, body_len) = decode_plist_header(&h);
            assert_eq!(body_len, 0);
        }

        #[test]
        fn encode_decode_round_trip_via_buffer() {
            // Write a frame into a Vec<u8>, slice off the header, decode back.
            let src = build_connect_request(17, 9753);
            let (header, body) = encode_plist_frame(&src, 2).unwrap();

            let mut header_arr = [0u8; HEADER_SIZE];
            header_arr.copy_from_slice(&header);
            let (tag, body_len) = decode_plist_header(&header_arr);
            assert_eq!(tag, 2);
            assert_eq!(body_len, body.len());

            let parsed = plist::Value::from_reader(std::io::Cursor::new(body)).unwrap();
            let d = parsed.as_dictionary().unwrap();
            assert_eq!(d.get("MessageType").unwrap().as_string(), Some("Connect"));
            assert_eq!(
                d.get("DeviceID").unwrap().as_unsigned_integer(),
                Some(17u64)
            );
        }

        #[test]
        fn plist_parse_rejects_malformed_bytes() {
            let garbage = b"not a plist at all";
            let result = plist::Value::from_reader(std::io::Cursor::new(garbage.to_vec()));
            assert!(result.is_err(), "garbage should not parse");
        }

        // UNTESTABLE: PHYS — UnixStream::connect(USBMUXD_SOCKET) in
        // connect_device() at line 15 and query_device_name_inner() at
        // line 58. Real usbmuxd socket only available on macOS with an
        // iOS device paired.
        //
        // UNTESTABLE: PHYS — lockdownd big-endian 4-byte framing in
        // lockdown_get_value() at line 97. Requires a live lockdownd
        // session over the usbmuxd tunnel.
    }
}

// Re-export for macOS
#[cfg(target_os = "macos")]
pub use imp::*;

// Stub for non-macOS platforms
#[cfg(not(target_os = "macos"))]
pub mod imp {
    pub async fn connect_device(
        _device_id: u32,
        _port: u16,
    ) -> Result<tokio::net::TcpStream, Box<dyn std::error::Error + Send + Sync>> {
        Err("usbmuxd is only available on macOS".into())
    }
}

#[cfg(not(target_os = "macos"))]
pub use imp::*;
