//! Persisted STEP payload encoding (roadmap §3.3): the feature stores the
//! source STEP text deflate-compressed + base64 inside the project JSON, so
//! a `.waffle` file is self-contained and the import replays on rebuild.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use std::io::{Read, Write};

/// The encoding tag written into the feature params. Bump only with a
/// decoder that accepts both.
pub const STEP_BLOB_ENCODING: &str = "deflate-base64";

/// Compress and encode STEP text for persistence.
pub fn encode_step_blob(step_text: &str) -> String {
    let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing into a Vec cannot fail.
    let _ = enc.write_all(step_text.as_bytes());
    let bytes = enc.finish().unwrap_or_default();
    B64.encode(bytes)
}

/// Decode a persisted blob back to STEP text.
pub fn decode_step_blob(encoding: &str, data: &str) -> Result<String, String> {
    if encoding != STEP_BLOB_ENCODING {
        return Err(format!("unknown STEP blob encoding '{encoding}'"));
    }
    let bytes = B64
        .decode(data)
        .map_err(|e| format!("STEP blob base64 decode failed: {e}"))?;
    let mut out = String::new();
    flate2::read::DeflateDecoder::new(bytes.as_slice())
        .read_to_string(&mut out)
        .map_err(|e| format!("STEP blob inflate failed: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_round_trip() {
        let text = include_str!("../tests/fixtures/cube.step");
        let blob = encode_step_blob(text);
        assert!(blob.len() < text.len(), "deflate should shrink STEP text");
        let back = decode_step_blob(STEP_BLOB_ENCODING, &blob).unwrap();
        assert_eq!(back, text);
    }

    #[test]
    fn unknown_encoding_is_loud() {
        assert!(decode_step_blob("gzip", "abc").is_err());
    }

    #[test]
    fn corrupt_data_is_loud() {
        assert!(decode_step_blob(STEP_BLOB_ENCODING, "!!!not-base64!!!").is_err());
    }
}
