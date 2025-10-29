// TODO: Unimplemented control message events (not yet needed for basic relay interop testing):
// - SubscribeUpdate (parsed/created)
// - PublishNamespaceDone (parsed/created)
// - PublishNamespaceCancel (parsed/created)
// - TrackStatus, TrackStatusOk, TrackStatusError (parsed/created)
// - SubscribeNamespace, SubscribeNamespaceOk, SubscribeNamespaceError, UnsubscribeNamespace (parsed/created)
// - Fetch, FetchOk, FetchError, FetchCancel (parsed/created)
// - Publish, PublishOk, PublishError, PublishDone (parsed/created)
// - MaxRequestId (parsed/created)
// - RequestsBlocked (parsed/created)
//
// TODO: Unimplemented data plane events (from draft-pardue-moq-qlog-moq-events):
// - stream_type_set (when stream type becomes known)
// - object_datagram_created/parsed
// - object_datagram_status_created/parsed
// - fetch_header_created/parsed
// - fetch_object_created/parsed
//
// TODO: stream_id field currently uses placeholder value (0)
// - Need to plumb actual QUIC stream IDs through web_transport abstractions
// - This would enable correlation between QUIC qlog and MoQ mlog events

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{coding, data, message, setup};

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

// Helper functions to create vector of string pairs from KVPs
fn key_value_pairs_to_vec(kvps: &coding::KeyValuePairs) -> Vec<(String, String)> {
    kvps.0
        .iter()
        .map(|(k, v)| (k.to_string(), format!("{:?}", v.value)))
        .collect()
}

fn create_control_message_event(
    time: f64,
    stream_id: u64,
    is_parsed: bool,
    msg_type: &str,
    message: JsonValue,
) -> Event {
    if is_parsed {
        Event {
            time,
            name: "moqt:control_message_parsed".to_string(),
            data: EventData::ControlMessageParsed(ControlMessageParsed {
                stream_id,
                message_type: msg_type.to_string(),
                message,
            }),
        }
    } else {
        Event {
            time,
            name: "moqt:control_message_created".to_string(),
            data: EventData::ControlMessageCreated(ControlMessageCreated {
                stream_id,
                message_type: msg_type.to_string(),
                message,
            }),
        }
    }
}

/// Create a control_message_parsed event for CLIENT_SETUP
pub fn client_setup_parsed(time: f64, stream_id: u64, msg: &setup::Client) -> Event {
    let versions: Vec<String> = msg.versions.0.iter().map(|v| format!("{:?}", v)).collect();
    create_control_message_event(
        time,
        stream_id,
        true,
        "client_setup",
        json!(
        {
            "number_of_supported_versions": msg.versions.0.len(),
            "supported_versions": versions,
            "parameters": key_value_pairs_to_vec(&msg.params),
        }),
    )
}

/// Create a control_message_created event for SERVER_SETUP
pub fn server_setup_created(time: f64, stream_id: u64, msg: &setup::Server) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "server_setup",
        json!(
        {
            "selected_version": format!("{:?}", msg.version),
            "parameters": key_value_pairs_to_vec(&msg.params),
        }),
    )
}

/// Helper to convert SUBSCRIBE message to JSON
fn subscribe_to_json(msg: &message::Subscribe) -> JsonValue {
    let mut json = json!({
        "subscribe_id": msg.id,
        "track_namespace": msg.track_namespace.to_string(),
        "track_name": &msg.track_name,
        "subscriber_priority": msg.subscriber_priority,
        "group_order": format!("{:?}", msg.group_order),
        "filter_type": format!("{:?}", msg.filter_type),
        "parameters": key_value_pairs_to_vec(&msg.params),
    });

    // Add optional fields based on filter type
    if let Some(start_loc) = &msg.start_location {
        json["start_group"] = json!(start_loc.group_id);
        json["start_object"] = json!(start_loc.object_id);
    }
    if let Some(end_group) = msg.end_group_id {
        json["end_group"] = json!(end_group);
    }

    json
}

/// Create a control_message_parsed event for SUBSCRIBE
pub fn subscribe_parsed(time: f64, stream_id: u64, msg: &message::Subscribe) -> Event {
    create_control_message_event(time, stream_id, true, "subscribe", subscribe_to_json(msg))
}

/// Create a control_message_created event for SUBSCRIBE
pub fn subscribe_created(time: f64, stream_id: u64, msg: &message::Subscribe) -> Event {
    create_control_message_event(time, stream_id, false, "subscribe", subscribe_to_json(msg))
}

/// Helper to convert SUBSCRIBE_OK message to JSON
fn subscribe_ok_to_json(msg: &message::SubscribeOk) -> JsonValue {
    let mut json = json!({
        "subscribe_id": msg.id,
        "track_alias": msg.track_alias,
        "expires": msg.expires,
        "group_order": format!("{:?}", msg.group_order),
        "content_exists": msg.content_exists,
        "parameters": key_value_pairs_to_vec(&msg.params),
    });

    // Add optional largest_location fields if content exists
    if msg.content_exists {
        if let Some(largest) = &msg.largest_location {
            json["largest_group_id"] = json!(largest.group_id);
            json["largest_object_id"] = json!(largest.object_id);
        }
    }

    json
}

/// Create a control_message_parsed event for SUBSCRIBE_OK
pub fn subscribe_ok_parsed(time: f64, stream_id: u64, msg: &message::SubscribeOk) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "subscribe_ok",
        subscribe_ok_to_json(msg),
    )
}

/// Create a control_message_created event for SUBSCRIBE_OK
pub fn subscribe_ok_created(time: f64, stream_id: u64, msg: &message::SubscribeOk) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "subscribe_ok",
        subscribe_ok_to_json(msg),
    )
}

/// Helper to convert SUBSCRIBE_ERROR message to JSON
fn subscribe_error_to_json(msg: &message::SubscribeError) -> JsonValue {
    json!({
        "subscribe_id": msg.id,
        "error_code": msg.error_code,
        "reason_phrase": &msg.reason_phrase.0,
    })
}

/// Create a control_message_parsed event for SUBSCRIBE_ERROR
pub fn subscribe_error_parsed(time: f64, stream_id: u64, msg: &message::SubscribeError) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "subscribe_error",
        subscribe_error_to_json(msg),
    )
}

/// Create a control_message_created event for SUBSCRIBE_ERROR
pub fn subscribe_error_created(time: f64, stream_id: u64, msg: &message::SubscribeError) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "subscribe_error",
        subscribe_error_to_json(msg),
    )
}

/// Helper to convert PUBLISH_NAMESPACE message to JSON
fn publish_namespace_to_json(msg: &message::PublishNamespace) -> JsonValue {
    json!({
        "request_id": msg.id,
        "track_namespace": msg.track_namespace.to_string(),
        "parameters": key_value_pairs_to_vec(&msg.params),
    })
}

/// Create a control_message_parsed event for PUBLISH_NAMESPACE (was ANNOUNCE in earlier drafts)
pub fn publish_namespace_parsed(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespace,
) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "publish_namespace",
        publish_namespace_to_json(msg),
    )
}

/// Create a control_message_created event for PUBLISH_NAMESPACE
pub fn publish_namespace_created(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespace,
) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "publish_namespace",
        publish_namespace_to_json(msg),
    )
}

/// Helper to convert PUBLISH_NAMESPACE_OK message to JSON
fn publish_namespace_ok_to_json(msg: &message::PublishNamespaceOk) -> JsonValue {
    json!({
        "request_id": msg.id,
    })
}

/// Create a control_message_parsed event for PUBLISH_NAMESPACE_OK (was ANNOUNCE_OK)
pub fn publish_namespace_ok_parsed(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceOk,
) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "publish_namespace_ok",
        publish_namespace_ok_to_json(msg),
    )
}

/// Create a control_message_created event for PUBLISH_NAMESPACE_OK
pub fn publish_namespace_ok_created(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceOk,
) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "publish_namespace_ok",
        publish_namespace_ok_to_json(msg),
    )
}

/// Helper to convert PUBLISH_NAMESPACE_ERROR message to JSON
fn publish_namespace_error_to_json(msg: &message::PublishNamespaceError) -> JsonValue {
    json!({
        "request_id": msg.id,
        "error_code": msg.error_code,
        "reason_phrase": &msg.reason_phrase.0,
    })
}

/// Create a control_message_parsed event for PUBLISH_NAMESPACE_ERROR (was ANNOUNCE_ERROR)
pub fn publish_namespace_error_parsed(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceError,
) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "publish_namespace_error",
        publish_namespace_error_to_json(msg),
    )
}

/// Create a control_message_created event for PUBLISH_NAMESPACE_ERROR
pub fn publish_namespace_error_created(
    time: f64,
    stream_id: u64,
    msg: &message::PublishNamespaceError,
) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "publish_namespace_error",
        publish_namespace_error_to_json(msg),
    )
}

/// Create a control_message_parsed event for UNSUBSCRIBE
pub fn unsubscribe_parsed(time: f64, stream_id: u64, msg: &message::Unsubscribe) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "unsubscribe",
        json!({
            "subscribe_id": msg.id,
        }),
    )
}

/// Create a control_message_created event for UNSUBSCRIBE
pub fn unsubscribe_created(time: f64, stream_id: u64, msg: &message::Unsubscribe) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "unsubscribe",
        json!({
            "subscribe_id": msg.id,
        }),
    )
}

/// Create a control_message_parsed event for GOAWAY
pub fn go_away_parsed(time: f64, stream_id: u64, msg: &message::GoAway) -> Event {
    create_control_message_event(
        time,
        stream_id,
        true,
        "goaway",
        json!({
                    "new_session_uri": &msg.uri.0,
        }),
    )
}

/// Create a control_message_created event for GOAWAY
pub fn go_away_created(time: f64, stream_id: u64, msg: &message::GoAway) -> Event {
    create_control_message_event(
        time,
        stream_id,
        false,
        "goaway",
        json!({
                    "new_session_uri": &msg.uri.0,
        }),
    )
}

// Data plane events

/// Helper to convert SubgroupHeader to JSON
fn subgroup_header_to_json(header: &data::SubgroupHeader) -> JsonValue {
    let mut json = json!({
        "track_alias": header.track_alias,
        "group_id": header.group_id,
        "publisher_priority": header.publisher_priority,
        "header_type": format!("{:?}", header.header_type),
    });

    if let Some(subgroup_id) = header.subgroup_id {
        json["subgroup_id"] = json!(subgroup_id);
    }

    json
}

/// Create a subgroup_header_parsed event
pub fn subgroup_header_parsed(time: f64, stream_id: u64, header: &data::SubgroupHeader) -> Event {
    Event {
        time,
        name: "moqt:subgroup_header_parsed".to_string(),
        data: EventData::SubgroupHeaderParsed(SubgroupHeaderParsed {
            stream_id,
            header: subgroup_header_to_json(header),
        }),
    }
}

/// Create a subgroup_header_created event
pub fn subgroup_header_created(time: f64, stream_id: u64, header: &data::SubgroupHeader) -> Event {
    Event {
        time,
        name: "moqt:subgroup_header_created".to_string(),
        data: EventData::SubgroupHeaderCreated(SubgroupHeaderCreated {
            stream_id,
            header: subgroup_header_to_json(header),
        }),
    }
}

/// Helper to convert SubgroupObject to JSON
fn subgroup_object_to_json(
    group_id: u64,
    subgroup_id: u64,
    object_id: u64,
    object: &data::SubgroupObject,
) -> JsonValue {
    let mut object_data = json!({
        "group_id": group_id,
        "subgroup_id": subgroup_id,
        "object_id": object_id,
        "extension_headers_length": 0,
        "object_payload_length": object.payload_length,
    });

    if let Some(status) = object.status {
        object_data["object_status"] = json!(format!("{:?}", status));
    }

    object_data
}

/// Create a subgroup_object_parsed event
pub fn subgroup_object_parsed(
    time: f64,
    stream_id: u64,
    group_id: u64,
    subgroup_id: u64,
    object_id: u64,
    object: &data::SubgroupObject,
) -> Event {
    Event {
        time,
        name: "moqt:subgroup_object_parsed".to_string(),
        data: EventData::SubgroupObjectParsed(SubgroupObjectParsed {
            stream_id,
            object: subgroup_object_to_json(group_id, subgroup_id, object_id, object),
        }),
    }
}

/// Create a subgroup_object_created event
pub fn subgroup_object_created(
    time: f64,
    stream_id: u64,
    group_id: u64,
    subgroup_id: u64,
    object_id: u64,
    object: &data::SubgroupObject,
) -> Event {
    Event {
        time,
        name: "moqt:subgroup_object_created".to_string(),
        data: EventData::SubgroupObjectCreated(SubgroupObjectCreated {
            stream_id,
            object: subgroup_object_to_json(group_id, subgroup_id, object_id, object),
        }),
    }
}

/// Helper to convert SubgroupObject to JSON
fn subgroup_object_ext_to_json(
    group_id: u64,
    subgroup_id: u64,
    object_id: u64,
    object: &data::SubgroupObjectExt,
) -> JsonValue {
    // TODO encode extension headers
    let mut object_data = json!({
        "group_id": group_id,
        "subgroup_id": subgroup_id,
        "object_id": object_id,
        "extension_headers_length": object.extension_headers.0.len(),
        "object_payload_length": object.payload_length,
    });

    if let Some(status) = object.status {
        object_data["object_status"] = json!(format!("{:?}", status));
    }

    object_data
}

/// Create a subgroup_object_parsed event (with extensions)
pub fn subgroup_object_ext_parsed(
    time: f64,
    stream_id: u64,
    group_id: u64,
    subgroup_id: u64,
    object_id: u64,
    object: &data::SubgroupObjectExt,
) -> Event {
    Event {
        time,
        name: "moqt:subgroup_object_parsed".to_string(),
        data: EventData::SubgroupObjectParsed(SubgroupObjectParsed {
            stream_id,
            object: subgroup_object_ext_to_json(group_id, subgroup_id, object_id, object),
        }),
    }
}

/// Create a subgroup_object_created event (with extensions)
pub fn subgroup_object_ext_created(
    time: f64,
    stream_id: u64,
    group_id: u64,
    subgroup_id: u64,
    object_id: u64,
    object: &data::SubgroupObjectExt,
) -> Event {
    Event {
        time,
        name: "moqt:subgroup_object_created".to_string(),
        data: EventData::SubgroupObjectCreated(SubgroupObjectCreated {
            stream_id,
            object: subgroup_object_ext_to_json(group_id, subgroup_id, object_id, object),
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
