//! Length-bucket padding for encrypted transport payloads.

use crate::{Result, SyncError};

const HEADER_BYTES: usize = 8;
const MIN_BUCKET_BYTES: usize = 64;
const MAX_BUCKET_BYTES: usize = 64 * 1024 * 1024;

pub fn pad_to_bucket(payload: &[u8]) -> Result<Vec<u8>> {
    let bucket = bucket_size(payload.len())?;
    let mut padded = vec![0_u8; bucket];
    padded[..HEADER_BYTES].copy_from_slice(&(payload.len() as u64).to_le_bytes());
    padded[HEADER_BYTES..HEADER_BYTES + payload.len()].copy_from_slice(payload);
    if HEADER_BYTES + payload.len() < padded.len() {
        getrandom::fill(&mut padded[HEADER_BYTES + payload.len()..])
            .map_err(|_| SyncError::Crypto)?;
    }
    Ok(padded)
}

fn bucket_size(payload_len: usize) -> Result<usize> {
    let required = payload_len
        .checked_add(HEADER_BYTES)
        .ok_or_else(|| SyncError::Invalid("payload is too large".into()))?;
    let bucket = required
        .max(MIN_BUCKET_BYTES)
        .checked_next_power_of_two()
        .filter(|bucket| *bucket <= MAX_BUCKET_BYTES)
        .ok_or_else(|| SyncError::Invalid("payload exceeds transport limit".into()))?;
    Ok(bucket)
}

pub fn unpad(padded: &[u8]) -> Result<&[u8]> {
    if !(MIN_BUCKET_BYTES..=MAX_BUCKET_BYTES).contains(&padded.len())
        || !padded.len().is_power_of_two()
    {
        return Err(SyncError::Invalid("invalid padded envelope size".into()));
    }
    let length = usize::try_from(u64::from_le_bytes(
        padded[..HEADER_BYTES]
            .try_into()
            .map_err(|_| SyncError::Invalid("missing length prefix".into()))?,
    ))
    .map_err(|_| SyncError::Invalid("padded payload length exceeds this platform".into()))?;
    let end = HEADER_BYTES
        .checked_add(length)
        .filter(|end| *end <= padded.len())
        .ok_or_else(|| SyncError::Invalid("invalid padded payload length".into()))?;
    Ok(&padded[HEADER_BYTES..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hides_exact_length_and_rejects_tampered_header() {
        let padded = pad_to_bucket(b"123456").unwrap();
        assert_eq!(padded.len(), 64);
        assert_eq!(unpad(&padded).unwrap(), b"123456");
        let mut tampered = padded;
        tampered[..8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(unpad(&tampered).is_err());
        assert!(bucket_size(usize::MAX).is_err());
        assert!(unpad(&[0; 32]).is_err());
    }
}
