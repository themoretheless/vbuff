use std::collections::BTreeMap;
use std::fmt;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use zeroize::Zeroize;

use super::IntegrationContractError;

type HmacSha256 = Hmac<Sha256>;
const MAX_WEBHOOK_TTL_MS: u64 = 5 * 60 * 1_000;
const MAX_WEBHOOK_BODY_BYTES: usize = 1_024 * 1_024;
const MAX_WEBHOOK_ENDPOINTS: usize = 4_096;

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
        if key.iter().all(|byte| *byte == 0) {
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
        if !valid_endpoint_id(endpoint_id)
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

    fn verify(&self, signed: &SignedWebhookEvent) -> Result<(), IntegrationContractError> {
        let payload = serde_json::to_vec(&signed.event)
            .map_err(|_| IntegrationContractError::InvalidField)?;
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| IntegrationContractError::InvalidField)?;
        mac.update(b"vbuff-webhook-v1");
        mac.update(&payload);
        mac.verify_slice(&signed.signature)
            .map_err(|_| IntegrationContractError::InvalidField)
    }

    fn signature(&self, event: &WebhookEvent) -> Result<[u8; 32], IntegrationContractError> {
        let payload =
            serde_json::to_vec(event).map_err(|_| IntegrationContractError::InvalidField)?;
        let mut mac = HmacSha256::new_from_slice(&self.key)
            .map_err(|_| IntegrationContractError::InvalidField)?;
        mac.update(b"vbuff-webhook-v1");
        mac.update(&payload);
        Ok(mac.finalize().into_bytes().into())
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

#[derive(Clone, Copy, Debug)]
struct WebhookReplayState {
    last_event_id: u64,
    expires_at_ms: u64,
}

#[derive(Clone, Default)]
pub struct WebhookReplayWindow {
    last_event_by_endpoint: BTreeMap<[u8; 32], WebhookReplayState>,
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
    pub fn verify_and_accept(
        &mut self,
        signer: &WebhookSigner,
        expected_endpoint_id: &str,
        signed: &SignedWebhookEvent,
        body: &[u8],
        now_ms: u64,
    ) -> Result<(), IntegrationContractError> {
        signer.verify(signed)?;
        self.last_event_by_endpoint
            .retain(|_, state| state.expires_at_ms > now_ms);
        let event = signed.event;
        if now_ms >= event.expires_at_ms {
            return Err(IntegrationContractError::Expired);
        }
        let previous = self
            .last_event_by_endpoint
            .get(&event.endpoint_hash)
            .copied();
        if !valid_endpoint_id(expected_endpoint_id)
            || event.endpoint_hash != *blake3::hash(expected_endpoint_id.as_bytes()).as_bytes()
            || now_ms < event.issued_at_ms
            || event.endpoint_hash.iter().all(|byte| *byte == 0)
            || event.body_hash.iter().all(|byte| *byte == 0)
            || event.event_id == 0
            || event.expires_at_ms <= event.issued_at_ms
            || event.expires_at_ms - event.issued_at_ms > MAX_WEBHOOK_TTL_MS
            || body.len() > MAX_WEBHOOK_BODY_BYTES
            || event.body_hash != *blake3::hash(body).as_bytes()
            || previous.is_some_and(|state| event.event_id <= state.last_event_id)
            || (!self
                .last_event_by_endpoint
                .contains_key(&event.endpoint_hash)
                && self.last_event_by_endpoint.len() >= MAX_WEBHOOK_ENDPOINTS)
        {
            return Err(IntegrationContractError::InvalidField);
        }
        self.last_event_by_endpoint.insert(
            event.endpoint_hash,
            WebhookReplayState {
                last_event_id: event.event_id,
                expires_at_ms: previous.map_or(event.expires_at_ms, |state| {
                    state.expires_at_ms.max(event.expires_at_ms)
                }),
            },
        );
        Ok(())
    }
}

fn valid_endpoint_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
