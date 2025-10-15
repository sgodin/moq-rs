use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{data, message, setup};

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

    #[serde(rename = "subgroup_header_parsed")]
    SubgroupHeaderParsed(SubgroupHeaderParsed),

    #[serde(rename = "subgroup_header_created")]
    SubgroupHeaderCreated(SubgroupHeaderCreated),

    #[serde(rename = "subgroup_object_parsed")]
    SubgroupObjectParsed(SubgroupObjectParsed),

    #[serde(rename = "subgroup_object_created")]
    SubgroupObjectCreated(SubgroupObjectCreated),

    #[serde(rename = "loglevel")]
    LogLevel(LogLevelEvent),
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

/// Subgroup header parsed event (data plane)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgroupHeaderParsed {
    pub stream_id: u64,

    /// Header-specific fields
    #[serde(flatten)]
    pub header: JsonValue,
}

/// Subgroup header created event (data plane)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgroupHeaderCreated {
    pub stream_id: u64,

    /// Header-specific fields
    #[serde(flatten)]
    pub header: JsonValue,
}

/// Subgroup object parsed event (data plane)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgroupObjectParsed {
    pub stream_id: u64,

    /// Object-specific fields
    #[serde(flatten)]
    pub object: JsonValue,
}

/// Subgroup object created event (data plane)
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubgroupObjectCreated {
    pub stream_id: u64,

    /// Object-specific fields
    #[serde(flatten)]
    pub object: JsonValue,
}

/// LogLevel event for flexible logging (qlog loglevel schema)
/// See: https://www.ietf.org/archive/id/draft-ietf-quic-qlog-main-schema-12.html#name-loglevel-events
#[serde_with::skip_serializing_none]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogLevelEvent {
    pub message: String,
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

/// Create a control_message_parsed event for SUBSCRIBE
pub fn subscribe_parsed(time: f64, stream_id: u64, msg: &message::Subscribe) -> Event {
    let mut message = json!({
        "subscribe_id": msg.id,
        "track_alias": msg.id, // In SUBSCRIBE, the id field serves as the track_alias
        "track_namespace": msg.track_namespace.to_string(),
        "track_name": &msg.track_name,
        "subscriber_priority": msg.subscriber_priority,
        "group_order": format!("{:?}", msg.group_order),
        "filter_type": format!("{:?}", msg.filter_type),
        "number_of_parameters": msg.params.0.len(),
    });

    // Add optional fields based on filter type
    if let Some(start_loc) = &msg.start_location {
        message["start_group"] = json!(start_loc.group_id);
        message["start_object"] = json!(start_loc.object_id);
    }
    if let Some(end_group) = msg.end_group_id {
        message["end_group"] = json!(end_group);
    }

    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "subscribe".to_string(),
            message,
        }),
    }
}

/// Create a control_message_created event for SUBSCRIBE
pub fn subscribe_created(time: f64, stream_id: u64, msg: &message::Subscribe) -> Event {
    let mut message = json!({
        "subscribe_id": msg.id,
        "track_alias": msg.id,
        "track_namespace": msg.track_namespace.to_string(),
        "track_name": &msg.track_name,
        "subscriber_priority": msg.subscriber_priority,
        "group_order": format!("{:?}", msg.group_order),
        "filter_type": format!("{:?}", msg.filter_type),
        "number_of_parameters": msg.params.0.len(),
    });

    if let Some(start_loc) = &msg.start_location {
        message["start_group"] = json!(start_loc.group_id);
        message["start_object"] = json!(start_loc.object_id);
    }
    if let Some(end_group) = msg.end_group_id {
        message["end_group"] = json!(end_group);
    }

    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "subscribe".to_string(),
            message,
        }),
    }
}

/// Create a control_message_parsed event for SUBSCRIBE_OK
pub fn subscribe_ok_parsed(time: f64, stream_id: u64, msg: &message::SubscribeOk) -> Event {
    let mut message = json!({
        "subscribe_id": msg.id,
        "expires": msg.expires,
        "group_order": format!("{:?}", msg.group_order),
        "content_exists": msg.content_exists,
        "number_of_parameters": msg.params.0.len(),
    });

    // Add optional largest_location fields if content exists
    if msg.content_exists {
        if let Some(largest) = &msg.largest_location {
            message["largest_group_id"] = json!(largest.group_id);
            message["largest_object_id"] = json!(largest.object_id);
        }
    }

    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "subscribe_ok".to_string(),
            message,
        }),
    }
}

/// Create a control_message_created event for SUBSCRIBE_OK
pub fn subscribe_ok_created(time: f64, stream_id: u64, msg: &message::SubscribeOk) -> Event {
    let mut message = json!({
        "subscribe_id": msg.id,
        "expires": msg.expires,
        "group_order": format!("{:?}", msg.group_order),
        "content_exists": msg.content_exists,
        "number_of_parameters": msg.params.0.len(),
    });

    if msg.content_exists {
        if let Some(largest) = &msg.largest_location {
            message["largest_group_id"] = json!(largest.group_id);
            message["largest_object_id"] = json!(largest.object_id);
        }
    }

    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "subscribe_ok".to_string(),
            message,
        }),
    }
}

/// Create a control_message_parsed event for SUBSCRIBE_ERROR
pub fn subscribe_error_parsed(time: f64, stream_id: u64, msg: &message::SubscribeError) -> Event {
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "subscribe_error".to_string(),
            message: json!({
                "subscribe_id": msg.id,
                "error_code": msg.error_code,
                "reason_phrase": &msg.reason_phrase.0,
            }),
        }),
    }
}

/// Create a control_message_created event for SUBSCRIBE_ERROR
pub fn subscribe_error_created(time: f64, stream_id: u64, msg: &message::SubscribeError) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "subscribe_error".to_string(),
            message: json!({
                "subscribe_id": msg.id,
                "error_code": msg.error_code,
                "reason_phrase": &msg.reason_phrase.0,
            }),
        }),
    }
}

/// Create a control_message_parsed event for PUBLISH_NAMESPACE (was ANNOUNCE in earlier drafts)
pub fn publish_namespace_parsed(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespace,
) -> Event {
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "publish_namespace".to_string(),
            message: json!({
                "request_id": msg.id,
                "track_namespace": msg.track_namespace.to_string(),
                "number_of_parameters": msg.params.0.len(),
            }),
        }),
    }
}

/// Create a control_message_created event for PUBLISH_NAMESPACE
pub fn publish_namespace_created(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespace,
) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "publish_namespace".to_string(),
            message: json!({
                "request_id": msg.id,
                "track_namespace": msg.track_namespace.to_string(),
                "number_of_parameters": msg.params.0.len(),
            }),
        }),
    }
}

/// Create a control_message_parsed event for PUBLISH_NAMESPACE_OK (was ANNOUNCE_OK)
pub fn publish_namespace_ok_parsed(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceOk,
) -> Event {
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "publish_namespace_ok".to_string(),
            message: json!({
                "request_id": msg.id,
            }),
        }),
    }
}

/// Create a control_message_created event for PUBLISH_NAMESPACE_OK
pub fn publish_namespace_ok_created(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceOk,
) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "publish_namespace_ok".to_string(),
            message: json!({
                "request_id": msg.id,
            }),
        }),
    }
}

/// Create a control_message_parsed event for PUBLISH_NAMESPACE_ERROR (was ANNOUNCE_ERROR)
pub fn publish_namespace_error_parsed(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceError,
) -> Event {
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "publish_namespace_error".to_string(),
            message: json!({
                "request_id": msg.id,
                "error_code": msg.error_code,
                "reason_phrase": &msg.reason_phrase.0,
            }),
        }),
    }
}

/// Create a control_message_created event for PUBLISH_NAMESPACE_ERROR
pub fn publish_namespace_error_created(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceError,
) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "publish_namespace_error".to_string(),
            message: json!({
                "request_id": msg.id,
                "error_code": msg.error_code,
                "reason_phrase": &msg.reason_phrase.0,
            }),
        }),
    }
}

/// Create a control_message_parsed event for UNSUBSCRIBE
pub fn unsubscribe_parsed(time: f64, stream_id: u64, msg: &message::Unsubscribe) -> Event {
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "unsubscribe".to_string(),
            message: json!({
                "subscribe_id": msg.id,
            }),
        }),
    }
}

/// Create a control_message_created event for UNSUBSCRIBE
pub fn unsubscribe_created(time: f64, stream_id: u64, msg: &message::Unsubscribe) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "unsubscribe".to_string(),
            message: json!({
                "subscribe_id": msg.id,
            }),
        }),
    }
}

/// Create a control_message_parsed event for GOAWAY
pub fn go_away_parsed(time: f64, stream_id: u64, msg: &message::GoAway) -> Event {
    Event {
        time,
        name: "moqt:control_message_parsed".to_string(),
        data: EventData::ControlMessageParsed(ControlMessageParsed {
            stream_id,
            message_type: "goaway".to_string(),
            message: json!({
                "new_session_uri": &msg.uri.0,
            }),
        }),
    }
}

/// Create a control_message_created event for GOAWAY
pub fn go_away_created(time: f64, stream_id: u64, msg: &message::GoAway) -> Event {
    Event {
        time,
        name: "moqt:control_message_created".to_string(),
        data: EventData::ControlMessageCreated(ControlMessageCreated {
            stream_id,
            message_type: "goaway".to_string(),
            message: json!({
                "new_session_uri": &msg.uri.0,
            }),
        }),
    }
}

// Data plane events

/// Create a subgroup_header_parsed event
pub fn subgroup_header_parsed(time: f64, stream_id: u64, header: &data::SubgroupHeader) -> Event {
    let mut header_data = json!({
        "track_alias": header.track_alias,
        "group_id": header.group_id,
        "publisher_priority": header.publisher_priority,
        "header_type": format!("{:?}", header.header_type),
    });

    if let Some(subgroup_id) = header.subgroup_id {
        header_data["subgroup_id"] = json!(subgroup_id);
    }

    Event {
        time,
        name: "moqt:subgroup_header_parsed".to_string(),
        data: EventData::SubgroupHeaderParsed(SubgroupHeaderParsed {
            stream_id,
            header: header_data,
        }),
    }
}

/// Create a subgroup_header_created event
pub fn subgroup_header_created(time: f64, stream_id: u64, header: &data::SubgroupHeader) -> Event {
    let mut header_data = json!({
        "track_alias": header.track_alias,
        "group_id": header.group_id,
        "publisher_priority": header.publisher_priority,
        "header_type": format!("{:?}", header.header_type),
    });

    if let Some(subgroup_id) = header.subgroup_id {
        header_data["subgroup_id"] = json!(subgroup_id);
    }

    Event {
        time,
        name: "moqt:subgroup_header_created".to_string(),
        data: EventData::SubgroupHeaderCreated(SubgroupHeaderCreated {
            stream_id,
            header: header_data,
        }),
    }
}

/// Create a subgroup_object_parsed event
pub fn subgroup_object_parsed(time: f64, stream_id: u64, object: &data::SubgroupObject) -> Event {
    let mut object_data = json!({
        "object_id_delta": object.object_id_delta,
        "payload_length": object.payload_length,
    });

    if let Some(status) = object.status {
        object_data["status"] = json!(format!("{:?}", status));
    }

    Event {
        time,
        name: "moqt:subgroup_object_parsed".to_string(),
        data: EventData::SubgroupObjectParsed(SubgroupObjectParsed {
            stream_id,
            object: object_data,
        }),
    }
}

/// Create a subgroup_object_created event
pub fn subgroup_object_created(time: f64, stream_id: u64, object: &data::SubgroupObject) -> Event {
    let mut object_data = json!({
        "object_id_delta": object.object_id_delta,
        "payload_length": object.payload_length,
    });

    if let Some(status) = object.status {
        object_data["status"] = json!(format!("{:?}", status));
    }

    Event {
        time,
        name: "moqt:subgroup_object_created".to_string(),
        data: EventData::SubgroupObjectCreated(SubgroupObjectCreated {
            stream_id,
            object: object_data,
        }),
    }
}

/// Create a subgroup_object_parsed event (with extensions)
pub fn subgroup_object_ext_parsed(
    time: f64,
    stream_id: u64,
    object: &data::SubgroupObjectExt,
) -> Event {
    let mut object_data = json!({
        "object_id_delta": object.object_id_delta,
        "payload_length": object.payload_length,
        "number_of_extension_headers": object.extension_headers.0.len(),
    });

    if let Some(status) = object.status {
        object_data["status"] = json!(format!("{:?}", status));
    }

    Event {
        time,
        name: "moqt:subgroup_object_parsed".to_string(),
        data: EventData::SubgroupObjectParsed(SubgroupObjectParsed {
            stream_id,
            object: object_data,
        }),
    }
}

/// Create a subgroup_object_created event (with extensions)
pub fn subgroup_object_ext_created(
    time: f64,
    stream_id: u64,
    object: &data::SubgroupObjectExt,
) -> Event {
    let mut object_data = json!({
        "object_id_delta": object.object_id_delta,
        "payload_length": object.payload_length,
        "number_of_extension_headers": object.extension_headers.0.len(),
    });

    if let Some(status) = object.status {
        object_data["status"] = json!(format!("{:?}", status));
    }

    Event {
        time,
        name: "moqt:subgroup_object_created".to_string(),
        data: EventData::SubgroupObjectCreated(SubgroupObjectCreated {
            stream_id,
            object: object_data,
        }),
    }
}

// LogLevel events (generic logging)

/// Log levels for qlog loglevel events
#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
    Verbose,
}

impl LogLevel {
    fn as_str(&self) -> &'static str {
        match self {
            LogLevel::Fatal => "fatal",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Verbose => "verbose",
        }
    }
}

/// Create a loglevel event for flexible logging
///
/// # Arguments
/// * `time` - Timestamp in milliseconds since connection start
/// * `level` - Log level (debug, info, warn, error, fatal, verbose)
/// * `message` - Freeform message text with structured information
///
/// # Example
/// ```ignore
/// loglevel_event(
///     12.345,
///     LogLevel::Debug,
///     "object_queued: track_alias=1 group=5 subgroup=2 object=10 payload_len=1024"
/// )
/// ```
pub fn loglevel_event(time: f64, level: LogLevel, message: String) -> Event {
    Event {
        time,
        name: format!("loglevel:{}", level.as_str()),
        data: EventData::LogLevel(LogLevelEvent { message }),
    }
}
