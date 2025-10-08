use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::setup;

/// MoQ Transport event following qlog patterns
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Time in milliseconds since connection start
    pub time: f64,
    
    /// Event name in format "moqt:event_name"
    pub name: String,
    
    /// Event-specific data
    pub data: EventData,
}

/// Union of all MoQ Transport event types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event_type")]
pub enum EventData {
    #[serde(rename = "control_message_parsed")]
    ControlMessageParsed(ControlMessageParsed),
    
    #[serde(rename = "control_message_created")]
    ControlMessageCreated(ControlMessageCreated),
}

/// Control message parsed event (Section 4.2 of draft-pardue-moq-qlog-moq-events)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlMessageParsed {
    pub stream_id: u64,
    pub message_type: String,
    
    /// Message-specific fields
    #[serde(flatten)]
    pub message: JsonValue,
}

/// Control message created event (Section 4.1 of draft-pardue-moq-qlog-moq-events)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ControlMessageCreated {
    pub stream_id: u64,
    pub message_type: String,
    
    /// Message-specific fields
    #[serde(flatten)]
    pub message: JsonValue,
}

// Helper functions to create events for specific message types

/// Create a control_message_parsed event for CLIENT_SETUP
pub fn client_setup_parsed(time: f64, stream_id: u64, msg: &setup::Client) -> Event {
    let versions: Vec<String> = msg.versions.0.iter().map(|v| format!("{:?}", v)).collect();
    
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "client_setup".to_string(),
            message: json!({
                "number_of_supported_versions": msg.versions.0.len(),
                "supported_versions": versions,
                "number_of_parameters": msg.params.0.len(),
            }),
        }),
    }
}

/// Create a control_message_created event for SERVER_SETUP
pub fn server_setup_created(time: f64, stream_id: u64, msg: &setup::Server) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "server_setup".to_string(),
            message: json!({
                "selected_version": format!("{:?}", msg.version),
                "number_of_parameters": msg.params.0.len(),
            }),
        }),
    }
}
