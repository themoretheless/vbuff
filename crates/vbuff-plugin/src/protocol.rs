//! Versioned protocol for capability-scoped native plugin subprocesses.
//!
//! The host owns process launch, sandboxing, timeouts, and capability checks.
//! Plugins exchange bounded JSON frames over inherited local pipes; no plugin
//! code is loaded into the resident process.

use std::io::Read;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{PluginError, Result};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;
const LENGTH_PREFIX_BYTES: usize = std::mem::size_of::<u32>();

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostFrame {
    Welcome {
        protocol_version: u16,
        granted_manifest_hash: [u8; 32],
    },
    Invoke {
        request_id: u64,
        action_id: String,
        input: Vec<u8>,
    },
    Cancel {
        request_id: u64,
    },
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PluginFrame {
    Hello {
        plugin_id: String,
        protocol_version: u16,
        manifest_hash: [u8; 32],
    },
    Result {
        request_id: u64,
        output: Vec<u8>,
    },
    Rejected {
        request_id: u64,
        code: String,
    },
}

pub fn encode_frame<T: Serialize>(frame: &T) -> Result<Vec<u8>> {
    let payload =
        serde_json::to_vec(frame).map_err(|error| PluginError::Serialization(error.to_string()))?;
    if payload.len() > MAX_FRAME_BYTES {
        return Err(PluginError::InvalidInput(
            "plugin frame is too large".into(),
        ));
    }
    let length = u32::try_from(payload.len())
        .map_err(|_| PluginError::InvalidInput("plugin frame is too large".into()))?;
    let mut frame_bytes = Vec::with_capacity(LENGTH_PREFIX_BYTES + payload.len());
    frame_bytes.extend_from_slice(&length.to_be_bytes());
    frame_bytes.extend_from_slice(&payload);
    Ok(frame_bytes)
}

pub fn decode_frame<T: DeserializeOwned>(frame: &[u8]) -> Result<T> {
    let header: [u8; LENGTH_PREFIX_BYTES] = frame
        .get(..LENGTH_PREFIX_BYTES)
        .and_then(|header| header.try_into().ok())
        .ok_or_else(|| PluginError::InvalidInput("plugin frame header is truncated".into()))?;
    let payload_len = u32::from_be_bytes(header) as usize;
    if payload_len > MAX_FRAME_BYTES {
        return Err(PluginError::InvalidInput(
            "plugin frame is too large".into(),
        ));
    }
    let expected_len = LENGTH_PREFIX_BYTES
        .checked_add(payload_len)
        .ok_or_else(|| PluginError::InvalidInput("plugin frame length overflow".into()))?;
    if frame.len() != expected_len {
        return Err(PluginError::InvalidInput(
            "plugin frame length does not match its payload".into(),
        ));
    }
    serde_json::from_slice(&frame[LENGTH_PREFIX_BYTES..])
        .map_err(|error| PluginError::Serialization(error.to_string()))
}

/// Read exactly one length-prefixed frame without allocating beyond the wire cap.
pub fn read_frame<R: Read, T: DeserializeOwned>(reader: &mut R) -> Result<T> {
    let mut header = [0_u8; LENGTH_PREFIX_BYTES];
    reader
        .read_exact(&mut header)
        .map_err(|_| PluginError::InvalidInput("plugin frame header is truncated".into()))?;
    let payload_len = u32::from_be_bytes(header) as usize;
    if payload_len > MAX_FRAME_BYTES {
        return Err(PluginError::InvalidInput(
            "plugin frame is too large".into(),
        ));
    }
    let mut payload = vec![0_u8; payload_len];
    reader
        .read_exact(&mut payload)
        .map_err(|_| PluginError::InvalidInput("plugin frame payload is truncated".into()))?;
    serde_json::from_slice(&payload).map_err(|error| PluginError::Serialization(error.to_string()))
}

pub fn protocol_hash() -> [u8; 32] {
    *blake3::hash(b"vbuff-native-plugin-protocol-v1-len32be-json-pipe").as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_frames_are_tagged_and_bounded() {
        let frame = PluginFrame::Hello {
            plugin_id: "dev.vbuff.sample".into(),
            protocol_version: PROTOCOL_VERSION,
            manifest_hash: [7; 32],
        };
        let encoded = encode_frame(&frame).unwrap();
        assert!(
            std::str::from_utf8(&encoded[LENGTH_PREFIX_BYTES..])
                .unwrap()
                .contains("\"hello\"")
        );
        assert_eq!(decode_frame::<PluginFrame>(&encoded).unwrap(), frame);
        assert_ne!(protocol_hash(), [0; 32]);

        let oversized = HostFrame::Invoke {
            request_id: 1,
            action_id: "transform".into(),
            input: vec![0; MAX_FRAME_BYTES],
        };
        assert!(encode_frame(&oversized).is_err());
    }

    #[test]
    fn stream_decoder_separates_frames_and_rejects_untrusted_lengths() {
        let first = PluginFrame::Rejected {
            request_id: 7,
            code: "denied".into(),
        };
        let second = PluginFrame::Rejected {
            request_id: 8,
            code: "cancelled".into(),
        };
        let mut stream = encode_frame(&first).unwrap();
        stream.extend(encode_frame(&second).unwrap());
        let mut cursor = std::io::Cursor::new(stream);
        assert_eq!(read_frame::<_, PluginFrame>(&mut cursor).unwrap(), first);
        assert_eq!(read_frame::<_, PluginFrame>(&mut cursor).unwrap(), second);

        let oversized = u32::try_from(MAX_FRAME_BYTES + 1).unwrap().to_be_bytes();
        assert!(read_frame::<_, PluginFrame>(&mut oversized.as_slice()).is_err());
        assert!(decode_frame::<PluginFrame>(&[0, 0, 0, 2, b'{']).is_err());
    }
}
