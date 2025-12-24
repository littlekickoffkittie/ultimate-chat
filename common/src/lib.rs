use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageType {
    Chat,
    System,
    UserJoin,
    UserLeave,
    PrivateMessage,
    RoomChange,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub username: String,
    pub content: String,
    pub room: String,
    pub timestamp: DateTime<Utc>,
    pub msg_type: MessageType,
    pub recipient: Option<String>,
}

impl ChatMessage {
    pub fn new(username: String, content: String, room: String, msg_type: MessageType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            username,
            content,
            room,
            timestamp: Utc::now(),
            msg_type,
            recipient: None,
        }
    }

    pub fn chat(username: String, content: String, room: String) -> Self {
        Self::new(username, content, room, MessageType::Chat)
    }

    pub fn system(content: String, room: String) -> Self {
        Self::new("System".to_string(), content, room, MessageType::System)
    }

    pub fn private(username: String, recipient: String, content: String) -> Self {
        let mut msg = Self::new(username, content, "private".to_string(), MessageType::PrivateMessage);
        msg.recipient = Some(recipient);
        msg
    }

    pub fn error(content: String) -> Self {
        Self::new("Error".to_string(), content, "global".to_string(), MessageType::Error)
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap()
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    pub fn format_time(&self) -> String {
        self.timestamp.format("%H:%M").to_string()
    }
}

// Request struct for initial connection/handshake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Handshake {
    pub username: String,
}
