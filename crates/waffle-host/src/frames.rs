//! The `waffle-host/1` wire format (`specs/waffle_server_mode.md` §2.4, §3.4).
//!
//! One frame is a JSON header with an optional binary payload:
//!
//! ```text
//! u32 BE header_len | u32 BE payload_len | header (JSON, UTF-8) | payload
//! ```
//!
//! The header always carries a string `type`. The payload is empty for every
//! S4 frame (`ready`, `tool`, `progress`, `result`, `cancel`, `bye`); the
//! viewer protocol (P-D) rides mesh blobs in it. Length-prefixed rather than
//! newline-delimited so a binary payload never needs escaping and a reader
//! never scans for a delimiter through megabytes of geometry.

use std::io::{self, Read, Write};

use serde_json::Value;

/// The protocol the host speaks; the relay refuses any other in `ready`.
pub const PROTOCOL: &str = "waffle-host/1";

/// A header larger than this is a protocol violation, not a big document:
/// the largest legitimate header is a `tool` frame carrying an
/// `import_step` payload (capped by the tool at its own inline limit).
pub const MAX_HEADER_BYTES: u32 = 64 * 1024 * 1024;
/// Viewer blobs (P-D) are the only payloads; a single one never exceeds this.
pub const MAX_PAYLOAD_BYTES: u32 = 256 * 1024 * 1024;

/// One decoded frame.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub header: Value,
    pub payload: Vec<u8>,
}

impl Frame {
    /// The header's `type`, or `""` when it has none (the reader already
    /// refused a header that is not an object).
    pub fn kind(&self) -> &str {
        self.header
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

/// Encode one frame into `out` as a single write, so two threads writing to
/// the same locked stream never interleave halves of a frame.
pub fn write_frame(out: &mut impl Write, header: &Value, payload: &[u8]) -> io::Result<()> {
    let header_bytes = serde_json::to_vec(header)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let header_len = u32::try_from(header_bytes.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame header too large"))?;
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "frame payload too large"))?;
    let mut buf = Vec::with_capacity(8 + header_bytes.len() + payload.len());
    buf.extend_from_slice(&header_len.to_be_bytes());
    buf.extend_from_slice(&payload_len.to_be_bytes());
    buf.extend_from_slice(&header_bytes);
    buf.extend_from_slice(payload);
    out.write_all(&buf)?;
    out.flush()
}

/// Decode the next frame from `input`. `Ok(None)` is a clean end of stream
/// (EOF exactly on a frame boundary); EOF inside a frame is an error, as is
/// a header that is not a JSON object with a string `type`.
pub fn read_frame(input: &mut impl Read) -> io::Result<Option<Frame>> {
    let mut lens = [0u8; 8];
    // A clean end of stream has NO bytes left; EOF after the first byte of a
    // prefix is a cut-off frame. `read_exact` cannot tell the two apart, so
    // the first byte is read on its own.
    let mut first = 0usize;
    while first == 0 {
        match input.read(&mut lens[..1]) {
            Ok(0) => return Ok(None),
            Ok(n) => first = n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    input.read_exact(&mut lens[1..])?;
    let header_len = u32::from_be_bytes([lens[0], lens[1], lens[2], lens[3]]);
    let payload_len = u32::from_be_bytes([lens[4], lens[5], lens[6], lens[7]]);
    if header_len > MAX_HEADER_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame header of {header_len} bytes exceeds {MAX_HEADER_BYTES}"),
        ));
    }
    if payload_len > MAX_PAYLOAD_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("frame payload of {payload_len} bytes exceeds {MAX_PAYLOAD_BYTES}"),
        ));
    }
    let mut header_bytes = vec![0u8; header_len as usize];
    input.read_exact(&mut header_bytes)?;
    let mut payload = vec![0u8; payload_len as usize];
    input.read_exact(&mut payload)?;
    let header: Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("frame header: {e}")))?;
    if !header.get("type").is_some_and(Value::is_string) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame header must be an object with a string `type`",
        ));
    }
    Ok(Some(Frame { header, payload }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_frame_round_trips_with_and_without_a_payload() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &json!({"type": "ready", "n": 1}), b"").unwrap();
        write_frame(&mut buf, &json!({"type": "viewer"}), b"\x00\x01binary").unwrap();
        let mut cursor = io::Cursor::new(buf);
        let first = read_frame(&mut cursor).unwrap().unwrap();
        assert_eq!(first.kind(), "ready");
        assert_eq!(first.header["n"], 1);
        assert!(first.payload.is_empty());
        let second = read_frame(&mut cursor).unwrap().unwrap();
        assert_eq!(second.payload, b"\x00\x01binary");
        assert!(read_frame(&mut cursor).unwrap().is_none(), "clean EOF");
    }

    #[test]
    fn a_truncated_frame_and_a_typeless_header_are_errors() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &json!({"type": "tool", "id": "a"}), b"").unwrap();
        buf.truncate(buf.len() - 3);
        let err = read_frame(&mut io::Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);

        let mut buf = Vec::new();
        write_frame(&mut buf, &json!({"id": "a"}), b"").unwrap();
        let err = read_frame(&mut io::Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn an_oversized_prefix_is_refused_before_allocating() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&(MAX_HEADER_BYTES + 1).to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        let err = read_frame(&mut io::Cursor::new(buf)).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
