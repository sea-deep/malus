//! LSP-style length-delimited framing and protocol encoding/decoding.
//!
//! Frames follow the LSP standard:
//! ```text
//! Content-Length: <byte_length>\r\n
//! \r\n
//! <exact JSON bytes>
//! ```

use serde::{Serialize, de::DeserializeOwned};
use std::io;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub const DEFAULT_MAX_PAYLOAD_BYTES: usize = 16 * 1024 * 1024; // 16 MB
pub const MAX_HEADER_BYTES: usize = 8 * 1024; // 8 KB

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FrameError {
    #[error("Missing Content-Length header")]
    MissingContentLength,
    #[error("Malformed Content-Length: {0}")]
    MalformedContentLength(String),
    #[error("Duplicate Content-Length header encountered")]
    DuplicateContentLength,
    #[error("Header section exceeds maximum allowed size ({MAX_HEADER_BYTES} bytes)")]
    HeaderTooLarge,
    #[error("Payload exceeds maximum size: {length} bytes (max {max} bytes)")]
    PayloadTooLarge { length: usize, max: usize },
    #[error("Header is not valid UTF-8: {0}")]
    InvalidHeaderUtf8(#[from] std::str::Utf8Error),
    #[error("Unexpected EOF while reading frame")]
    UnexpectedEof,
    #[error("I/O error: {0}")]
    Io(String),
    #[error("JSON serialization error: {0}")]
    Serialization(String),
}

impl From<io::Error> for FrameError {
    fn from(err: io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<serde_json::Error> for FrameError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err.to_string())
    }
}

/// Encode a payload slice into an LSP-style framed byte buffer.
pub fn encode_frame(payload: &[u8]) -> Vec<u8> {
    let header = format!("Content-Length: {}\r\n\r\n", payload.len());
    let mut buf = Vec::with_capacity(header.len() + payload.len());
    buf.extend_from_slice(header.as_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Encode a serializable value into an LSP-style framed byte buffer.
pub fn encode_message<T: Serialize>(msg: &T) -> Result<Vec<u8>, FrameError> {
    let json_bytes = serde_json::to_vec(msg)?;
    Ok(encode_frame(&json_bytes))
}

/// Decode a JSON message from raw payload bytes.
pub fn decode_message<T: DeserializeOwned>(payload: &[u8]) -> Result<T, FrameError> {
    serde_json::from_slice(payload).map_err(|e| FrameError::Serialization(e.to_string()))
}

/// Attempt to decode a single frame from the buffer.
///
/// If a full frame is available, it drains the frame from `buf` and returns `Ok(Some(payload))`.
/// If more data is needed, returns `Ok(None)`.
/// If framing is malformed, exceeds bounds, or contains duplicate headers, returns `Err(FrameError)`.
pub fn decode_frame(buf: &mut Vec<u8>, max_payload: usize) -> Result<Option<Vec<u8>>, FrameError> {
    // Look for "\r\n\r\n"
    let (header_end, delimiter_len) = match buf.windows(4).position(|w| w == b"\r\n\r\n") {
        Some(pos) => (pos, 4),
        None => {
            // Also tolerate "\n\n"
            if let Some(pos) = buf.windows(2).position(|w| w == b"\n\n") {
                (pos, 2)
            } else {
                // Delimiter not found yet: check header size bound immediately
                if buf.len() > MAX_HEADER_BYTES {
                    return Err(FrameError::HeaderTooLarge);
                }
                return Ok(None);
            }
        }
    };

    if header_end > MAX_HEADER_BYTES {
        return Err(FrameError::HeaderTooLarge);
    }

    let header_str = std::str::from_utf8(&buf[..header_end])?;
    let mut content_length: Option<usize> = None;

    for line in header_str.lines() {
        let line = line.trim();
        if let Some((key, val)) = line.split_once(':') {
            if !key.trim().eq_ignore_ascii_case("content-length") {
                continue;
            }
            if content_length.is_some() {
                return Err(FrameError::DuplicateContentLength);
            }
            let trimmed_val = val.trim();
            // Validate that value is non-empty and consists purely of ASCII digits (rejecting negative, signs, trailing text)
            if trimmed_val.is_empty() || !trimmed_val.chars().all(|c| c.is_ascii_digit()) {
                return Err(FrameError::MalformedContentLength(trimmed_val.to_string()));
            }
            let len = trimmed_val
                .parse::<usize>()
                .map_err(|_| FrameError::MalformedContentLength(trimmed_val.to_string()))?;
            content_length = Some(len);
        }
    }

    let payload_len = content_length.ok_or(FrameError::MissingContentLength)?;

    if payload_len > max_payload {
        return Err(FrameError::PayloadTooLarge {
            length: payload_len,
            max: max_payload,
        });
    }

    let total_frame_len = header_end + delimiter_len + payload_len;
    if buf.len() < total_frame_len {
        return Ok(None); // Need more bytes
    }

    let payload_start = header_end + delimiter_len;
    let payload = buf[payload_start..total_frame_len].to_vec();

    // Drain frame from buffer
    buf.drain(..total_frame_len);

    Ok(Some(payload))
}

/// Asynchronously read one frame from an async stream into the supplied buffer.
/// Returns `Ok(None)` on clean EOF before any frame data begins.
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
    max_payload: usize,
) -> Result<Option<Vec<u8>>, FrameError> {
    loop {
        if let Some(frame) = decode_frame(buf, max_payload)? {
            return Ok(Some(frame));
        }

        let mut chunk = [0u8; 4096];
        let n = reader.read(&mut chunk).await?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None);
            } else {
                return Err(FrameError::UnexpectedEof);
            }
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

/// Asynchronously write a framed payload to an async stream.
pub async fn write_frame<W: AsyncWrite + Unpin>(
    writer: &mut W,
    payload: &[u8],
) -> Result<(), FrameError> {
    let framed = encode_frame(payload);
    writer.write_all(&framed).await?;
    writer.flush().await?;
    Ok(())
}

/// Asynchronously write a serializable message to an async stream.
pub async fn write_message<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    msg: &T,
) -> Result<(), FrameError> {
    let payload = serde_json::to_vec(msg)?;
    write_frame(writer, &payload).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_frame_encoding_and_decoding_roundtrip() {
        let payload = br#"{"action":"play"}"#;
        let framed = encode_frame(payload);

        let mut buf = framed.clone();
        let decoded = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .expect("Should decode full frame");

        assert_eq!(decoded, payload);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_partial_reads_fragmentation() {
        let payload = br#"{"query":"Acoustic Sunset","limit":20}"#;
        let framed = encode_frame(payload);

        let mut buf = Vec::new();
        for (i, &byte) in framed.iter().enumerate() {
            buf.push(byte);
            let res = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES).unwrap();
            if i + 1 < framed.len() {
                assert!(res.is_none(), "Should need more bytes at index {i}");
            } else {
                assert_eq!(res.unwrap(), payload);
            }
        }
        assert!(buf.is_empty());
    }

    #[test]
    fn test_multiple_frames_in_single_buffer() {
        let frame1 = encode_frame(br#"{"msg":1}"#);
        let frame2 = encode_frame(br#"{"msg":2}"#);
        let frame3 = encode_frame(br#"{"msg":3}"#);

        let mut combined = Vec::new();
        combined.extend_from_slice(&frame1);
        combined.extend_from_slice(&frame2);
        combined.extend_from_slice(&frame3);

        let d1 = decode_frame(&mut combined, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let d2 = decode_frame(&mut combined, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        let d3 = decode_frame(&mut combined, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();

        assert_eq!(d1, br#"{"msg":1}"#);
        assert_eq!(d2, br#"{"msg":2}"#);
        assert_eq!(d3, br#"{"msg":3}"#);
        assert!(combined.is_empty());
    }

    #[test]
    fn test_embedded_newlines_in_json_payload() {
        let payload = b"{\n  \"multiline\": \"hello\\nworld\",\r\n  \"status\": true\n}";
        let framed = encode_frame(payload);

        let mut buf = framed;
        let decoded = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .unwrap()
            .unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn test_malformed_missing_content_length() {
        let mut buf = b"Content-Type: application/json\r\n\r\n{}".to_vec();
        let err = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES).unwrap_err();
        assert_eq!(err, FrameError::MissingContentLength);
    }

    #[test]
    fn test_duplicate_content_length_rejected() {
        let mut buf = b"Content-Length: 10\r\nContent-Length: 10\r\n\r\n0123456789".to_vec();
        let err = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES).unwrap_err();
        assert_eq!(err, FrameError::DuplicateContentLength);
    }

    #[test]
    fn test_malformed_content_length_non_digits() {
        let cases = [
            b"Content-Length: abc\r\n\r\n".to_vec(),
            b"Content-Length: -10\r\n\r\n".to_vec(),
            b"Content-Length: 10abc\r\n\r\n".to_vec(),
            b"Content-Length: 10 20\r\n\r\n".to_vec(),
            b"Content-Length: \r\n\r\n".to_vec(),
        ];
        for mut buf in cases {
            let err = decode_frame(&mut buf, DEFAULT_MAX_PAYLOAD_BYTES).unwrap_err();
            assert!(matches!(err, FrameError::MalformedContentLength(_)));
        }
    }

    #[test]
    fn test_header_too_large_rejection() {
        let mut flood = vec![b'A'; MAX_HEADER_BYTES + 10];
        let err = decode_frame(&mut flood, DEFAULT_MAX_PAYLOAD_BYTES).unwrap_err();
        assert_eq!(err, FrameError::HeaderTooLarge);
    }

    #[test]
    fn test_payload_too_large_rejection() {
        let framed = encode_frame(&[0u8; 100]);
        let mut buf = framed;
        let err = decode_frame(&mut buf, 50).unwrap_err();
        assert!(matches!(err, FrameError::PayloadTooLarge { .. }));
    }

    #[tokio::test]
    async fn test_async_read_frame_eof_handling() {
        let mut empty_stream = Cursor::new(Vec::new());
        let mut buf = Vec::new();
        let res = read_frame(&mut empty_stream, &mut buf, DEFAULT_MAX_PAYLOAD_BYTES)
            .await
            .unwrap();
        assert!(res.is_none());

        let mut partial_stream = Cursor::new(b"Content-Length: 10\r\n\r\n123".to_vec());
        let mut buf2 = Vec::new();
        let err = read_frame(&mut partial_stream, &mut buf2, DEFAULT_MAX_PAYLOAD_BYTES)
            .await
            .unwrap_err();
        assert_eq!(err, FrameError::UnexpectedEof);
    }
}
