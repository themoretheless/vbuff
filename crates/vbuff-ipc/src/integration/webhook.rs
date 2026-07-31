use std::fmt;

use serde::{Deserialize, Serialize};
use vbuff_types::mac::{MacDomain, MacProof, hmac_proof};
use vbuff_types::validation::{all_zero, is_valid_identifier};
use zeroize::Zeroize;

use crate::replay::{MAX_REPLAY_ENTRIES, ReplayGuard};

use super::IntegrationContractError;

/// Frozen framing: this signature is handed to third-party endpoints whose
/// verifiers live outside this repository, so the preimage cannot gain a
/// terminator without a versioned signature header and a deprecation window
/// (`docs/domain-separation-convention.md` §7.2). It is unambiguous as it
/// stands, because exactly one variable-length part follows and it is last.
const WEBHOOK_DOMAIN: MacDomain = MacDomain::legacy_unterminated("vbuff-webhook-v1");

const MAX_WEBHOOK_TTL_MS: u64 = 5 * 60 * 1_000;
const MAX_WEBHOOK_BODY_BYTES: usize = 1_024 * 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventKind {
    ClipAdded,
    ClipPinned,
    ClipDeleted,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WebhookEvent {
    pub endpoint_hash: [u8; 32],
    pub event_id: u64,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub kind: WebhookEventKind,
    pub body_hash: [u8; 32],
}

impl fmt::Debug for WebhookEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebhookEvent")
            .field("endpoint_hash", &"[redacted]")
            .field("event_id", &self.event_id)
            .field("issued_at_ms", &self.issued_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("kind", &self.kind)
            .field("body_hash", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedWebhookEvent {
    pub event: WebhookEvent,
    signature: [u8; 32],
}

impl fmt::Debug for SignedWebhookEvent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedWebhookEvent")
            .field("event_id", &self.event.event_id)
            .field("issued_at_ms", &self.event.issued_at_ms)
            .field("expires_at_ms", &self.event.expires_at_ms)
            .field("kind", &self.event.kind)
            .field("body_hash", &"[redacted]")
            .field("signature", &"[redacted]")
            .finish()
    }
}

pub struct WebhookSigner {
    key: [u8; 32],
}

impl WebhookSigner {
    pub fn from_key(key: [u8; 32]) -> Result<Self, IntegrationContractError> {
        if all_zero(&key) {
            return Err(IntegrationContractError::InvalidField);
        }
        Ok(Self { key })
    }

    pub fn sign(
        &self,
        endpoint_id: &str,
        event_id: u64,
        issued_at_ms: u64,
        ttl_ms: u64,
        kind: WebhookEventKind,
        body: &[u8],
    ) -> Result<SignedWebhookEvent, IntegrationContractError> {
        if !is_valid_identifier(endpoint_id, 128)
            || event_id == 0
            || ttl_ms == 0
            || ttl_ms > MAX_WEBHOOK_TTL_MS
            || body.len() > MAX_WEBHOOK_BODY_BYTES
        {
            return Err(IntegrationContractError::InvalidField);
        }
        let event = WebhookEvent {
            endpoint_hash: *blake3::hash(endpoint_id.as_bytes()).as_bytes(),
            event_id,
            issued_at_ms,
            expires_at_ms: issued_at_ms
                .checked_add(ttl_ms)
                .ok_or(IntegrationContractError::InvalidField)?,
            kind,
            body_hash: *blake3::hash(body).as_bytes(),
        };
        let signature = self.signature(&event)?;
        Ok(SignedWebhookEvent { event, signature })
    }

    /// The one statement of what a webhook signature covers: the canonical
    /// JSON of the whole [`WebhookEvent`], and nothing else. Both
    /// [`Self::signature`] and [`Self::verify`] reach their tag through here,
    /// so a field added to `WebhookEvent` is covered on both paths or on
    /// neither - it can never be signed but left unverified.
    fn event_proof(&self, event: &WebhookEvent) -> Result<MacProof, IntegrationContractError> {
        let payload =
            serde_json::to_vec(event).map_err(|_| IntegrationContractError::InvalidField)?;
        Ok(hmac_proof(WEBHOOK_DOMAIN, &self.key, &[&payload]))
    }

    fn verify(&self, signed: &SignedWebhookEvent) -> Result<(), IntegrationContractError> {
        if self.event_proof(&signed.event)?.verify(&signed.signature) {
            Ok(())
        } else {
            Err(IntegrationContractError::InvalidField)
        }
    }

    fn signature(&self, event: &WebhookEvent) -> Result<[u8; 32], IntegrationContractError> {
        Ok(self.event_proof(event)?.finish())
    }
}

impl fmt::Debug for WebhookSigner {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("WebhookSigner([redacted])")
    }
}

impl Drop for WebhookSigner {
    fn drop(&mut self) {
        self.key.zeroize();
    }
}

/// Per-endpoint sequence window: each endpoint keeps the highest event id it
/// accepted, retained for as long as any event it accepted could still be
/// replayed.
#[derive(Clone)]
pub struct WebhookReplayWindow {
    last_event_by_endpoint: ReplayGuard<[u8; 32], u64>,
}

impl Default for WebhookReplayWindow {
    fn default() -> Self {
        Self {
            last_event_by_endpoint: ReplayGuard::new(MAX_REPLAY_ENTRIES),
        }
    }
}

impl fmt::Debug for WebhookReplayWindow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebhookReplayWindow")
            .field("endpoint_count", &self.last_event_by_endpoint.len())
            .finish()
    }
}

impl WebhookReplayWindow {
    /// Verifies a signed event and advances the endpoint's sequence watermark.
    ///
    /// The watermark is retained until the latest expiry among the events this
    /// endpoint accepted (the guard merges with `max`, never shortening), which
    /// is exactly the instant where the *last* of those events starts failing
    /// the freshness check above. So dropping the endpoint cannot resurrect
    /// any of them: while any accepted event is still replayable the watermark
    /// is present and refuses ids at or below it, and once the entry is gone
    /// every one of those events is past its expiry - permanently, because the
    /// guard's clock floor never moves backwards (see
    /// [`ReplayGuard::advance_to`]).
    pub fn verify_and_accept(
        &mut self,
        signer: &WebhookSigner,
        expected_endpoint_id: &str,
        signed: &SignedWebhookEvent,
        body: &[u8],
        now_ms: u64,
    ) -> Result<(), IntegrationContractError> {
        signer.verify(signed)?;
        let now_ms = self.last_event_by_endpoint.advance_to(now_ms);
        let event = signed.event;
        if now_ms >= event.expires_at_ms {
            return Err(IntegrationContractError::Expired);
        }
        let previous = self
            .last_event_by_endpoint
            .state(&event.endpoint_hash)
            .copied();
        if !is_valid_identifier(expected_endpoint_id, 128)
            || event.endpoint_hash != *blake3::hash(expected_endpoint_id.as_bytes()).as_bytes()
            || now_ms < event.issued_at_ms
            || all_zero(&event.endpoint_hash)
            || all_zero(&event.body_hash)
            || event.event_id == 0
            || event.expires_at_ms <= event.issued_at_ms
            || event.expires_at_ms - event.issued_at_ms > MAX_WEBHOOK_TTL_MS
            || body.len() > MAX_WEBHOOK_BODY_BYTES
            || event.body_hash != *blake3::hash(body).as_bytes()
            || previous.is_some_and(|last_event_id| event.event_id <= last_event_id)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        self.last_event_by_endpoint
            .record(event.endpoint_hash, event.event_id, event.expires_at_ms)
            .map_err(|_| IntegrationContractError::InvalidField)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hex, legacy_mac};

    /// Freeze test for the webhook signature.
    ///
    /// This is the one MAC in the crate whose verifiers are third-party code
    /// outside this repository. A failure here is not a local regression, it is
    /// every subscriber's signature check breaking at once, with no migration
    /// path we control. Changing these bytes needs a versioned signature
    /// header and a deprecation window, never an edit to this constant.
    #[test]
    fn webhook_mac_bytes_are_frozen_and_domain_bound() {
        let signer = WebhookSigner::from_key([7; 32]).unwrap();
        let event = WebhookEvent {
            endpoint_hash: [1; 32],
            event_id: 4,
            issued_at_ms: 100,
            expires_at_ms: 200,
            kind: WebhookEventKind::ClipAdded,
            body_hash: [2; 32],
        };
        let payload = serde_json::to_vec(&event).unwrap();
        let tag = signer.signature(&event).unwrap();

        assert_eq!(
            hex(&tag),
            "b3c791c09672b5836260a315313abdc7d513984ebbc2787eb8991022fc959e70"
        );
        // Byte-identical to the hand-rolled `mac.update(b"vbuff-webhook-v1")`
        // pair this replaced.
        assert_eq!(
            tag,
            legacy_mac(b"vbuff-webhook-v1", &signer.key, &[&payload])
        );
        assert!(
            signer
                .verify(&SignedWebhookEvent {
                    event,
                    signature: tag
                })
                .is_ok()
        );

        // The same label under the NUL convention is a different domain, and
        // its tag must not verify. This is what makes "add a terminator" a
        // format break rather than a tidy-up.
        let foreign =
            hmac_proof(MacDomain::new("vbuff-webhook-v1"), &signer.key, &[&payload]).finish();
        assert_ne!(foreign, tag);
        assert_eq!(
            signer.verify(&SignedWebhookEvent {
                event,
                signature: foreign
            }),
            Err(IntegrationContractError::InvalidField)
        );
    }

    #[test]
    fn webhook_signature_binds_monotonic_id_window_and_body() {
        let signer = WebhookSigner::from_key([7; 32]).unwrap();
        let event = signer
            .sign(
                "automation",
                1,
                100,
                1_000,
                WebhookEventKind::ClipAdded,
                b"opaque event body",
            )
            .unwrap();
        let mut window = WebhookReplayWindow::default();
        window
            .verify_and_accept(&signer, "automation", &event, b"opaque event body", 500)
            .unwrap();
        assert!(
            window
                .verify_and_accept(&signer, "automation", &event, b"opaque event body", 501,)
                .is_err()
        );
        let next = signer
            .sign(
                "automation",
                2,
                200,
                1_000,
                WebhookEventKind::ClipPinned,
                b"next body",
            )
            .unwrap();
        assert!(
            window
                .verify_and_accept(&signer, "automation", &next, b"tampered body", 500)
                .is_err()
        );
        assert!(
            WebhookReplayWindow::default()
                .verify_and_accept(&signer, "different-endpoint", &next, b"next body", 500,)
                .is_err()
        );
        assert!(!format!("{event:?}").contains("opaque event body"));
        assert!(!format!("{:?}", event.event).contains(&format!("{:?}", event.event.body_hash)));
        assert_eq!(
            format!("{window:?}"),
            "WebhookReplayWindow { endpoint_count: 1 }"
        );
        assert_eq!(
            WebhookSigner::from_key([0; 32]).err(),
            Some(IntegrationContractError::InvalidField)
        );
        assert_eq!(
            WebhookReplayWindow::default().verify_and_accept(
                &signer,
                "automation",
                &event,
                b"opaque event body",
                event.event.expires_at_ms,
            ),
            Err(IntegrationContractError::Expired)
        );

        let long_lived = signer
            .sign(
                "long-window",
                1,
                100,
                MAX_WEBHOOK_TTL_MS,
                WebhookEventKind::ClipAdded,
                b"first",
            )
            .unwrap();
        let short_lived = signer
            .sign(
                "long-window",
                2,
                200,
                100,
                WebhookEventKind::ClipPinned,
                b"second",
            )
            .unwrap();
        let mut durable_window = WebhookReplayWindow::default();
        durable_window
            .verify_and_accept(&signer, "long-window", &long_lived, b"first", 200)
            .unwrap();
        durable_window
            .verify_and_accept(&signer, "long-window", &short_lived, b"second", 250)
            .unwrap();
        assert!(
            durable_window
                .verify_and_accept(&signer, "long-window", &long_lived, b"first", 400)
                .is_err()
        );
    }

    fn event(
        signer: &WebhookSigner,
        endpoint: &str,
        event_id: u64,
        issued_at_ms: u64,
        ttl_ms: u64,
    ) -> SignedWebhookEvent {
        signer
            .sign(
                endpoint,
                event_id,
                issued_at_ms,
                ttl_ms,
                WebhookEventKind::ClipAdded,
                b"body",
            )
            .unwrap()
    }

    #[test]
    fn cleanup_keeps_rejecting_replays_inside_the_window() {
        let signer = WebhookSigner::from_key([11; 32]).unwrap();
        // Issued at 100, valid until 1_100.
        let first = event(&signer, "kept", 5, 100, 1_000);
        let mut window = WebhookReplayWindow::default();
        window
            .verify_and_accept(&signer, "kept", &first, b"body", 200)
            .unwrap();
        assert_eq!(window.last_event_by_endpoint.len(), 1);

        // Every verification prunes; a neighbouring endpoint coming and going
        // must not take the still-valid watermark with it.
        let neighbour = event(&signer, "transient", 1, 200, 1);
        window
            .verify_and_accept(&signer, "transient", &neighbour, b"body", 200)
            .unwrap();
        assert_eq!(
            window.verify_and_accept(&signer, "kept", &first, b"body", 1_099),
            Err(IntegrationContractError::InvalidField)
        );
        assert_eq!(window.last_event_by_endpoint.len(), 1);
    }

    #[test]
    fn endpoint_entries_are_evicted_once_they_leave_the_validity_window() {
        let signer = WebhookSigner::from_key([12; 32]).unwrap();
        let first = event(&signer, "aging", 5, 100, 1_000);
        let mut window = WebhookReplayWindow::default();
        window
            .verify_and_accept(&signer, "aging", &first, b"body", 200)
            .unwrap();

        // At expiry the watermark is dropped, and the event is refused on time
        // instead of on the sequence, so eviction weakens nothing.
        assert_eq!(
            window.verify_and_accept(&signer, "aging", &first, b"body", 1_100),
            Err(IntegrationContractError::Expired)
        );
        assert!(window.last_event_by_endpoint.is_empty());
    }

    #[test]
    fn rewound_clock_cannot_resurrect_an_evicted_endpoint() {
        let signer = WebhookSigner::from_key([13; 32]).unwrap();
        let first = event(&signer, "rewound", 5, 100, 1_000);
        let mut window = WebhookReplayWindow::default();
        window
            .verify_and_accept(&signer, "rewound", &first, b"body", 200)
            .unwrap();

        // Traffic on another endpoint carries the window past the first
        // event's expiry, so its watermark is gone; only the monotonic floor
        // stands between a rewound clock and a replay of event 5.
        let other = event(&signer, "other", 1, 2_000, 1_000);
        window
            .verify_and_accept(&signer, "other", &other, b"body", 2_000)
            .unwrap();
        assert_eq!(window.last_event_by_endpoint.len(), 1);

        assert_eq!(
            window.verify_and_accept(&signer, "rewound", &first, b"body", 200),
            Err(IntegrationContractError::Expired)
        );
    }

    #[test]
    fn saturated_endpoint_window_fails_closed() {
        let signer = WebhookSigner::from_key([14; 32]).unwrap();
        let mut window = WebhookReplayWindow::default();
        for index in 0..MAX_REPLAY_ENTRIES - 1 {
            let mut hash = [0_u8; 32];
            hash[..8].copy_from_slice(&(index as u64).to_be_bytes());
            window
                .last_event_by_endpoint
                .record(hash, 1, 1_000)
                .unwrap();
        }
        let known = event(&signer, "known", 1, 100, 1_000);
        window
            .verify_and_accept(&signer, "known", &known, b"body", 200)
            .unwrap();
        assert_eq!(window.last_event_by_endpoint.len(), MAX_REPLAY_ENTRIES);

        // A new endpoint cannot be recorded, so its event is refused rather
        // than accepted without a watermark.
        let newcomer = event(&signer, "newcomer", 1, 100, 1_000);
        assert_eq!(
            window.verify_and_accept(&signer, "newcomer", &newcomer, b"body", 200),
            Err(IntegrationContractError::InvalidField)
        );

        // An endpoint already inside the window keeps advancing while
        // saturated, otherwise saturation would freeze its sequence.
        let next = event(&signer, "known", 2, 300, 1_000);
        window
            .verify_and_accept(&signer, "known", &next, b"body", 300)
            .unwrap();

        // Once the saturating entries age out, new endpoints are admitted.
        window
            .verify_and_accept(
                &signer,
                "newcomer",
                &event(&signer, "newcomer", 1, 1_000, 1_000),
                b"body",
                1_000,
            )
            .unwrap();
        assert_eq!(window.last_event_by_endpoint.len(), 2);
    }
}
