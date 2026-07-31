use std::fmt;

use serde::{Deserialize, Serialize};

use vbuff_types::mac::{MacDomain, MacProof, hmac_proof};
use vbuff_types::validation::{all_zero, is_valid_identifier};

use crate::replay::{MAX_REPLAY_ENTRIES, ReplayGuard};

use super::IntegrationContractError;

/// Frozen framing: every part of the preimage below is fixed-width, so the
/// missing terminator cannot make two leases collide, and adding one would
/// invalidate leases held by a process already running against this build
/// (`docs/domain-separation-convention.md` §6.1, §7.3). New parts must stay
/// fixed-width or the domain has to be bumped to `-v2`.
const REMOTE_PASTE_DOMAIN: MacDomain = MacDomain::legacy_unterminated("vbuff-remote-paste-v1");

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemotePasteRequest {
    pub forwarded_socket: String,
    pub session_nonce: String,
    pub clip_id: String,
}

impl fmt::Debug for RemotePasteRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemotePasteRequest")
            .field("forwarded_socket_bytes", &self.forwarded_socket.len())
            .field("session_nonce", &"[redacted]")
            .field("clip_id", &"[redacted]")
            .finish()
    }
}

impl RemotePasteRequest {
    pub fn validate(&self) -> Result<(), IntegrationContractError> {
        if !valid_forwarded_socket(&self.forwarded_socket)
            || !is_valid_identifier(&self.session_nonce, 128)
            || !is_valid_identifier(&self.clip_id, 128)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(())
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemotePasteLease {
    pub request_hash: [u8; 32],
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    proof: [u8; 32],
}

impl fmt::Debug for RemotePasteLease {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemotePasteLease")
            .field("request_hash", &"[redacted]")
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("proof", &"[redacted]")
            .finish()
    }
}

impl RemotePasteLease {
    pub fn bind(
        request: &RemotePasteRequest,
        session_key: &[u8; 32],
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> Result<Self, IntegrationContractError> {
        request.validate()?;
        if all_zero(session_key) || ttl_ms == 0 || ttl_ms > 60_000 {
            return Err(IntegrationContractError::InvalidField);
        }
        let expires_at_ms = issued_at_ms
            .checked_add(ttl_ms)
            .ok_or(IntegrationContractError::InvalidField)?;
        let request_hash = request_hash(request)?;
        let proof = remote_proof(session_key, &request_hash, issued_at_ms, expires_at_ms)?.finish();
        Ok(Self {
            request_hash,
            issued_at_ms,
            expires_at_ms,
            proof,
        })
    }
}

/// One-shot window over remote paste leases, keyed by the hash of the request
/// the lease is bound to.
#[derive(Clone)]
pub struct RemoteReplayWindow {
    consumed: ReplayGuard<[u8; 32]>,
}

impl Default for RemoteReplayWindow {
    fn default() -> Self {
        Self {
            consumed: ReplayGuard::new(MAX_REPLAY_ENTRIES),
        }
    }
}

impl fmt::Debug for RemoteReplayWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteReplayWindow")
            .field("consumed_count", &self.consumed.len())
            .finish()
    }
}

impl RemoteReplayWindow {
    /// Verifies a lease and burns the request it is bound to.
    ///
    /// The entry is burned until the lease's own `expires_at_ms`, which is
    /// exactly the instant where the freshness check above starts answering
    /// [`IntegrationContractError::Expired`], so eviction cannot resurrect a
    /// lease: inside the window the replay hits the burned entry, and from the
    /// moment the entry is dropped the guard's monotonic clock floor keeps the
    /// lease expired even if the caller's clock is rewound (see
    /// [`ReplayGuard::advance_to`]).
    pub fn verify_and_consume(
        &mut self,
        lease: &RemotePasteLease,
        request: &RemotePasteRequest,
        session_key: &[u8; 32],
        now_ms: u64,
    ) -> Result<(), IntegrationContractError> {
        let now_ms = self.consumed.advance_to(now_ms);
        request.validate()?;
        if all_zero(session_key) {
            return Err(IntegrationContractError::InvalidField);
        }
        if now_ms < lease.issued_at_ms || now_ms >= lease.expires_at_ms {
            return Err(IntegrationContractError::Expired);
        }
        let request_hash = request_hash(request)?;
        if request_hash != lease.request_hash
            || self.consumed.contains(&request_hash)
            || !remote_proof(
                session_key,
                &request_hash,
                lease.issued_at_ms,
                lease.expires_at_ms,
            )
            .is_ok_and(|proof| proof.verify(&lease.proof))
        {
            return Err(IntegrationContractError::InvalidField);
        }
        self.consumed
            .burn(request_hash, lease.expires_at_ms)
            .map_err(|_| IntegrationContractError::InvalidField)
    }
}

fn request_hash(request: &RemotePasteRequest) -> Result<[u8; 32], IntegrationContractError> {
    serde_json::to_vec(request)
        .map(|bytes| *blake3::hash(&bytes).as_bytes())
        .map_err(|_| IntegrationContractError::InvalidField)
}

/// The one statement of what a remote-paste lease proof covers: the hash of
/// the request the lease is bound to plus its validity window, all
/// fixed-width. [`RemotePasteLease::bind`] and
/// [`RemoteReplayWindow::verify_and_consume`] both reach their tag through
/// here, so binding and verification cannot come to cover different fields.
/// The all-zero key is refused here so the refusal applies to both directions.
fn remote_proof(
    key: &[u8; 32],
    request_hash: &[u8; 32],
    issued_at_ms: u64,
    expires_at_ms: u64,
) -> Result<MacProof, IntegrationContractError> {
    if all_zero(key) {
        return Err(IntegrationContractError::InvalidField);
    }
    Ok(hmac_proof(
        REMOTE_PASTE_DOMAIN,
        key,
        &[
            request_hash,
            &issued_at_ms.to_be_bytes(),
            &expires_at_ms.to_be_bytes(),
        ],
    ))
}

fn valid_forwarded_socket(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && !value.contains("//")
        && !value
            .split('/')
            .any(|segment| matches!(segment, "." | ".."))
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/' | b':')
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hex, legacy_mac};

    const SESSION_KEY: [u8; 32] = [7; 32];

    fn paste_request(clip_id: &str) -> RemotePasteRequest {
        RemotePasteRequest {
            forwarded_socket: "localhost:/run/user/1000/vbuff.sock".into(),
            session_nonce: "nonce-1".into(),
            clip_id: clip_id.into(),
        }
    }

    /// Freeze test for the remote-paste lease proof, and the executable
    /// statement of which fields it covers.
    #[test]
    fn remote_paste_mac_bytes_are_frozen_and_cover_every_field() {
        let request_hash = [3_u8; 32];
        let tag = remote_proof(&SESSION_KEY, &request_hash, 100, 200)
            .unwrap()
            .finish();

        assert_eq!(
            hex(&tag),
            "1e258e3bd1480ef7cc0299fe6e1c618f1a66c15bd7f5aa4b333f6198ae0a9a34"
        );
        // Byte-identical to the hand-rolled
        // `mac.update(b"vbuff-remote-paste-v1")` pair this replaced.
        assert_eq!(
            tag,
            legacy_mac(
                b"vbuff-remote-paste-v1",
                &SESSION_KEY,
                &[
                    &request_hash,
                    &100_u64.to_be_bytes(),
                    &200_u64.to_be_bytes(),
                ]
            )
        );
        assert!(
            remote_proof(&SESSION_KEY, &request_hash, 100, 200)
                .unwrap()
                .verify(&tag)
        );

        // Every covered field is genuinely covered.
        for (hash, issued, expires) in [
            ([9_u8; 32], 100, 200),
            (request_hash, 101, 200),
            (request_hash, 100, 201),
        ] {
            assert!(
                !remote_proof(&SESSION_KEY, &hash, issued, expires)
                    .unwrap()
                    .verify(&tag)
            );
        }

        // The same label under the NUL convention is a foreign domain, as is
        // the sibling MCP lease domain over an identically shaped message.
        for foreign in [
            hmac_proof(
                MacDomain::new("vbuff-remote-paste-v1"),
                &SESSION_KEY,
                &[
                    &request_hash,
                    &100_u64.to_be_bytes(),
                    &200_u64.to_be_bytes(),
                ],
            )
            .finish(),
            legacy_mac(
                b"vbuff-mcp-lease-v1",
                &SESSION_KEY,
                &[
                    &request_hash,
                    &100_u64.to_be_bytes(),
                    &200_u64.to_be_bytes(),
                ],
            ),
        ] {
            assert_ne!(foreign, tag);
            assert!(
                !remote_proof(&SESSION_KEY, &request_hash, 100, 200)
                    .unwrap()
                    .verify(&foreign)
            );
        }

        // The all-zero key is refused on the verifying path too.
        assert_eq!(
            remote_proof(&[0; 32], &request_hash, 100, 200).err(),
            Some(IntegrationContractError::InvalidField)
        );
    }

    #[test]
    fn cleanup_keeps_rejecting_replays_inside_the_window() {
        let request = paste_request("clip-1");
        // Issued at 100, valid until 1_100.
        let lease = RemotePasteLease::bind(&request, &SESSION_KEY, 100, 1_000).unwrap();
        let mut window = RemoteReplayWindow::default();
        window
            .verify_and_consume(&lease, &request, &SESSION_KEY, 500)
            .unwrap();
        assert_eq!(window.consumed.len(), 1);

        // Every verification prunes; a neighbour coming and going must not take
        // the still-valid entry with it.
        let neighbour = paste_request("clip-2");
        let short_lived = RemotePasteLease::bind(&neighbour, &SESSION_KEY, 600, 1).unwrap();
        window
            .verify_and_consume(&short_lived, &neighbour, &SESSION_KEY, 600)
            .unwrap();
        assert_eq!(
            window.verify_and_consume(&lease, &request, &SESSION_KEY, 1_099),
            Err(IntegrationContractError::InvalidField)
        );
        assert_eq!(window.consumed.len(), 1);
    }

    #[test]
    fn entries_are_evicted_once_they_leave_the_validity_window() {
        let request = paste_request("clip-1");
        let lease = RemotePasteLease::bind(&request, &SESSION_KEY, 100, 1_000).unwrap();
        let mut window = RemoteReplayWindow::default();
        window
            .verify_and_consume(&lease, &request, &SESSION_KEY, 500)
            .unwrap();

        // At expiry the entry is dropped, and the lease is refused on time
        // instead of on the burned set, so eviction weakens nothing.
        assert_eq!(
            window.verify_and_consume(&lease, &request, &SESSION_KEY, 1_100),
            Err(IntegrationContractError::Expired)
        );
        assert!(window.consumed.is_empty());
    }

    #[test]
    fn rewound_clock_cannot_resurrect_an_evicted_lease() {
        let request = paste_request("clip-1");
        let lease = RemotePasteLease::bind(&request, &SESSION_KEY, 100, 1_000).unwrap();
        let mut window = RemoteReplayWindow::default();
        window
            .verify_and_consume(&lease, &request, &SESSION_KEY, 500)
            .unwrap();

        // Traffic carries the window past the lease's expiry, so its entry is
        // gone; only the monotonic floor stands between a rewound clock and a
        // second paste on the same lease.
        let later = paste_request("clip-2");
        let later_lease = RemotePasteLease::bind(&later, &SESSION_KEY, 2_000, 1_000).unwrap();
        window
            .verify_and_consume(&later_lease, &later, &SESSION_KEY, 2_000)
            .unwrap();
        assert_eq!(window.consumed.len(), 1);

        assert_eq!(
            window.verify_and_consume(&lease, &request, &SESSION_KEY, 500),
            Err(IntegrationContractError::Expired)
        );
    }

    #[test]
    fn saturated_replay_window_fails_closed() {
        let request = paste_request("clip-1");
        let lease = RemotePasteLease::bind(&request, &SESSION_KEY, 100, 1_000).unwrap();
        let mut window = RemoteReplayWindow::default();
        for index in 0..MAX_REPLAY_ENTRIES {
            let mut hash = [0_u8; 32];
            hash[..8].copy_from_slice(&(index as u64).to_be_bytes());
            window.consumed.burn(hash, 1_000).unwrap();
        }
        assert_eq!(
            window.verify_and_consume(&lease, &request, &SESSION_KEY, 500),
            Err(IntegrationContractError::InvalidField)
        );

        // Once the saturating entries age out, the window accepts again.
        let fresh = RemotePasteLease::bind(&request, &SESSION_KEY, 1_000, 1_000).unwrap();
        window
            .verify_and_consume(&fresh, &request, &SESSION_KEY, 1_000)
            .unwrap();
        assert_eq!(window.consumed.len(), 1);
    }
}
