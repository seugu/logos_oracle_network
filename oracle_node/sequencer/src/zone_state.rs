#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use lb_core::mantle::ops::channel::MsgId;
use logos_blockchain_zone_sdk::sequencer::InscriptionInfo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMessage {
    pub tx_uuid: Uuid,
    pub text: String,
}

impl AppMessage {
    pub fn new(text: String) -> Self {
        Self {
            tx_uuid: Uuid::new_v4(),
            text,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("AppMessage serialization should not fail")
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }
}

#[derive(Debug, Clone)]
pub struct Msg {
    pub msg_id: MsgId,
    pub text: String,
}

impl Msg {
    pub fn from_payload(msg_id: MsgId, payload: &[u8]) -> Self {
        let text = AppMessage::from_bytes(payload)
            .map_or_else(|| String::from_utf8_lossy(payload).into_owned(), |m| m.text);
        Self { msg_id, text }
    }
}

pub trait ZoneState: Send {
    fn on_published(&mut self, info: &InscriptionInfo);
    fn on_adopted(&mut self, adopted: &[InscriptionInfo]);
    /// Remove our orphaned entry from `published`. Caller is expected to
    /// auto-republish via `handle.publish_message`.
    fn on_orphaned(&mut self, msg_id: &MsgId);
    fn on_finalized(&mut self, inscriptions: &[InscriptionInfo]);

    fn published(&self) -> &[Msg];
    fn adopted(&self) -> &[Msg];
    fn finalized(&self) -> &[Msg];
}

/// In-memory implementation of [`ZoneState`].
#[derive(Default)]
pub struct InMemoryZoneState {
    published: Vec<Msg>,
    adopted: Vec<Msg>,
    finalized: Vec<Msg>,
}

impl ZoneState for InMemoryZoneState {
    fn on_published(&mut self, info: &InscriptionInfo) {
        self.published
            .push(Msg::from_payload(info.this_msg, &info.payload));
    }

    fn on_adopted(&mut self, adopted: &[InscriptionInfo]) {
        for info in adopted {
            if !self.adopted.iter().any(|m| m.msg_id == info.this_msg) {
                self.adopted
                    .push(Msg::from_payload(info.this_msg, &info.payload));
            }
        }
    }

    fn on_orphaned(&mut self, msg_id: &MsgId) {
        if let Some(i) = self.published.iter().position(|m| &m.msg_id == msg_id) {
            self.published.remove(i);
        }
    }

    fn on_finalized(&mut self, inscriptions: &[InscriptionInfo]) {
        for info in inscriptions {
            if let Some(i) = self
                .published
                .iter()
                .position(|m| m.msg_id == info.this_msg)
            {
                self.published.remove(i);
            } else if let Some(i) = self.adopted.iter().position(|m| m.msg_id == info.this_msg) {
                self.adopted.remove(i);
            }
            if !self.finalized.iter().any(|m| m.msg_id == info.this_msg) {
                self.finalized
                    .push(Msg::from_payload(info.this_msg, &info.payload));
            }
        }
    }

    fn published(&self) -> &[Msg] {
        &self.published
    }

    fn adopted(&self) -> &[Msg] {
        &self.adopted
    }

    fn finalized(&self) -> &[Msg] {
        &self.finalized
    }
}