use bytes::BytesMut;
use serde::Serialize;

use crate::message::{
    ClientFrame, ServerFrame, MAX_BUFFER_BYTES, MAX_FRAME_BYTES, PROTOCOL_VERSION,
};
use crate::ProtocolError;

/// Streaming decoder for length-prefixed frames.
#[derive(Debug, Default)]
pub struct FrameDecoder {
    buf: BytesMut,
}

impl FrameDecoder {
    pub fn new() -> Self {
        Self { buf: BytesMut::new() }
    }

    /// Push newly read bytes and return zero or more complete JSON bodies.
    pub fn push(&mut self, data: &[u8]) -> Result<Vec<Vec<u8>>, ProtocolError> {
        if self.buf.len().saturating_add(data.len()) > MAX_BUFFER_BYTES {
            return Err(ProtocolError::BufferFull);
        }
        self.buf.extend_from_slice(data);
        let mut frames = Vec::new();
        loop {
            if self.buf.len() < 4 {
                break;
            }
            let len_bytes: [u8; 4] =
                self.buf[..4].try_into().map_err(|_| ProtocolError::Malformed)?;
            let len = u32::from_be_bytes(len_bytes) as usize;
            if len > MAX_FRAME_BYTES {
                return Err(ProtocolError::FrameTooLarge);
            }
            if self.buf.len() < 4 + len {
                break;
            }
            let _ = self.buf.split_to(4);
            let body = self.buf.split_to(len);
            frames.push(body.to_vec());
        }
        Ok(frames)
    }
}

pub fn encode_frame(value: &impl Serialize) -> Result<Vec<u8>, ProtocolError> {
    let body = serde_json::to_vec(value).map_err(|_| ProtocolError::Malformed)?;
    if body.len() > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge);
    }
    let mut out = Vec::with_capacity(4 + body.len());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

pub fn decode_client(bytes: &[u8]) -> Result<ClientFrame, ProtocolError> {
    let frame: ClientFrame = serde_json::from_slice(bytes).map_err(|_| ProtocolError::Malformed)?;
    if frame.v != PROTOCOL_VERSION {
        return Err(ProtocolError::BadVersion(frame.v));
    }
    Ok(frame)
}

pub fn decode_server(bytes: &[u8]) -> Result<ServerFrame, ProtocolError> {
    let frame: ServerFrame = serde_json::from_slice(bytes).map_err(|_| ProtocolError::Malformed)?;
    if frame.v != PROTOCOL_VERSION {
        return Err(ProtocolError::BadVersion(frame.v));
    }
    Ok(frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{ClientMessage, ServerMessage};

    #[test]
    fn round_trip_and_split_reads() {
        let frame =
            ClientFrame { v: PROTOCOL_VERSION, id: 7, body: ClientMessage::Ping { nonce: 3 } };
        let bytes = encode_frame(&frame).unwrap();
        let mut dec = FrameDecoder::new();
        assert!(dec.push(&bytes[..3]).unwrap().is_empty());
        let out = dec.push(&bytes[3..]).unwrap();
        assert_eq!(out.len(), 1);
        let decoded = decode_client(&out[0]).unwrap();
        assert_eq!(decoded, frame);
    }

    #[test]
    fn rejects_oversized_length() {
        let mut dec = FrameDecoder::new();
        let mut prefix = (MAX_FRAME_BYTES as u32 + 1).to_be_bytes().to_vec();
        prefix.extend_from_slice(&[0, 0]);
        assert!(matches!(dec.push(&prefix), Err(ProtocolError::FrameTooLarge)));
    }

    #[test]
    fn rejects_unknown_fields() {
        let raw = br#"{"v":1,"id":1,"body":{"type":"ping","nonce":1,"extra":true}}"#;
        assert!(decode_client(raw).is_err());
    }

    #[test]
    fn server_error_encodes() {
        let frame = ServerFrame {
            v: PROTOCOL_VERSION,
            id: 1,
            body: ServerMessage::error("rejected", "no"),
        };
        let bytes = encode_frame(&frame).unwrap();
        let mut dec = FrameDecoder::new();
        let body = dec.push(&bytes).unwrap();
        assert_eq!(decode_server(&body[0]).unwrap(), frame);
    }
}
