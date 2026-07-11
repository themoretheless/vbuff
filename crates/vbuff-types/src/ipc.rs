//! Minimal control protocol used for liveness and single-instance handoff.

use serde::{Deserialize, Serialize};

/// Intent sent by a short-lived client to the resident process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientIntent {
    /// Summon and focus the resident popup.
    ShowPopup,
    /// Verify that the endpoint belongs to a responsive vbuff process.
    Ping,
}

/// Response to one control intent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerResponse {
    Ack,
    Pong,
    Rejected { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_contract_roundtrips_with_stable_tags() {
        let show = serde_json::to_string(&ClientIntent::ShowPopup).unwrap();
        assert_eq!(show, r#"{"type":"show_popup"}"#);
        assert_eq!(
            serde_json::from_str::<ClientIntent>(&show).unwrap(),
            ClientIntent::ShowPopup
        );

        for response in [
            ServerResponse::Ack,
            ServerResponse::Pong,
            ServerResponse::Rejected {
                message: "not ready".into(),
            },
        ] {
            let json = serde_json::to_string(&response).unwrap();
            assert_eq!(
                serde_json::from_str::<ServerResponse>(&json).unwrap(),
                response
            );
        }
    }
}
