use crate::coding::{Decode, DecodeError, Encode, EncodeError, KeyValuePairs, Location, TrackNamespace};
use crate::message::{GroupOrder, FetchType};

/// Sent by the subscriber to request to request a range
/// of already published objects within a track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fetch {
    /// The fetch request ID
    pub id: u64,

    /// Subscriber Priority
    pub subscriber_priority: u8,

    /// Object delivery order
    pub group_order: GroupOrder,

    /// Standalone fetch vs Relative Joining fetch vs Absolute Joining fetch
    pub fetch_type: FetchType,

    /// Track properties for Standalone fetch
    pub track_namespace: Option<TrackNamespace>,
    pub track_name: Option<String>,
    pub start_location: Option<Location>,
    pub end_location: Option<Location>,

    /// Joining properties for Relative Joining or Absolute Joining fetches.
    /// The request ID of the existing subscription to be joined.
    pub joining_id: Option<u64>,
    pub joining_start: Option<u64>,

    /// Optional parameters
    pub params: KeyValuePairs,
}

impl Decode for Fetch {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let id = u64::decode(r)?;

        let subscriber_priority = u8::decode(r)?;
        let group_order = GroupOrder::decode(r)?;

        let fetch_type = FetchType::decode(r)?;

        let track_namespace: Option<TrackNamespace>;
        let track_name: Option<String>;
        let start_location: Option<Location>;
        let end_location: Option<Location>;
        let joining_id: Option<u64>;
        let joining_start: Option<u64>;
        match fetch_type {
            FetchType::Standalone => {
                track_namespace = Some(TrackNamespace::decode(r)?);
                track_name = Some(String::decode(r)?);
                start_location = Some(Location::decode(r)?);
                end_location = Some(Location::decode(r)?);
                joining_id = None;
                joining_start = None;
            }
            FetchType::RelativeJoining | FetchType::AbsoluteJoining => {
                track_namespace = None;
                track_name = None;
                start_location = None;
                end_location = None;
                joining_id = Some(u64::decode(r)?);
                joining_start = Some(u64::decode(r)?);
            }
        };

        let params = KeyValuePairs::decode(r)?;

        Ok(Self {
            id,
            subscriber_priority,
            group_order,
            fetch_type,
            track_namespace,
            track_name,
            start_location,
            end_location,
            joining_id,
            joining_start,
            params,
        })
    }
}

impl Encode for Fetch {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.id.encode(w)?;

        self.subscriber_priority.encode(w)?;
        self.group_order.encode(w)?;

        self.fetch_type.encode(w)?;

        match self.fetch_type {
            FetchType::Standalone => {
                if let Some(namespace) = &self.track_namespace {
                    namespace.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
                if let Some(name) = &self.track_name {
                    name.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
                if let Some(start) = &self.start_location {
                    start.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
                if let Some(end) = &self.end_location {
                    end.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
            }
            FetchType::RelativeJoining | FetchType::AbsoluteJoining => {
                if let Some(id) = self.joining_id {
                    id.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
                if let Some(start) = self.joining_start {
                    start.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
            }
        };

        self.params.encode(w)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn encode_decode() {
        let mut buf = BytesMut::new();

        // One parameter for testing
        let mut kvps = KeyValuePairs::new();
        kvps.set_bytesvalue(123, vec![0x00, 0x01, 0x02, 0x03]);

        // FetchType = Standlone
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::Standalone,
            track_namespace: Some(TrackNamespace::from_utf8_path("test/path/to/resource")),
            track_name: Some("audiotrack".to_string()),
            start_location: Some(Location::new(34, 53)),
            end_location: Some(Location::new(34, 53)),
            joining_id: None,
            joining_start: None,
            params: kvps.clone(),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = Fetch::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // FetchType = RelativeJoining
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::RelativeJoining,
            track_namespace: None,
            track_name: None,
            start_location: None,
            end_location: None,
            joining_id: Some(382),
            joining_start: Some(3463),
            params: kvps.clone(),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = Fetch::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // FetchType = AbsoluteJoining
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::AbsoluteJoining,
            track_namespace: None,
            track_name: None,
            start_location: None,
            end_location: None,
            joining_id: Some(382),
            joining_start: Some(3463),
            params: kvps.clone(),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = Fetch::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn encode_missing_fields() {
        let mut buf = BytesMut::new();

        // FetchType = Standlone - missing track_namespace
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::Standalone,
            track_namespace: None,
            track_name: Some("audiotrack".to_string()),
            start_location: Some(Location::new(34, 53)),
            end_location: Some(Location::new(34, 53)),
            joining_id: None,
            joining_start: None,
            params: Default::default(),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // FetchType = Standlone - missing track_name
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::Standalone,
            track_namespace: Some(TrackNamespace::from_utf8_path("test/path/to/resource")),
            track_name: None,
            start_location: Some(Location::new(34, 53)),
            end_location: Some(Location::new(34, 53)),
            joining_id: None,
            joining_start: None,
            params: Default::default(),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // FetchType = Standlone - missing start_location
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::Standalone,
            track_namespace: Some(TrackNamespace::from_utf8_path("test/path/to/resource")),
            track_name: Some("audiotrack".to_string()),
            start_location: None,
            end_location: Some(Location::new(34, 53)),
            joining_id: None,
            joining_start: None,
            params: Default::default(),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // FetchType = Standlone - missing namespace
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::Standalone,
            track_namespace: Some(TrackNamespace::from_utf8_path("test/path/to/resource")),
            track_name: Some("audiotrack".to_string()),
            start_location: Some(Location::new(34, 53)),
            end_location: None,
            joining_id: None,
            joining_start: None,
            params: Default::default(),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // FetchType = AbsoluteJoining - missing joining_id
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::AbsoluteJoining,
            track_namespace: Some(TrackNamespace::from_utf8_path("test/path/to/resource")),
            track_name: Some("audiotrack".to_string()),
            start_location: Some(Location::new(34, 53)),
            end_location: None,
            joining_id: None,
            joining_start: None,
            params: Default::default(),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // FetchType = RelativeJoining - missing joining_start
        let msg = Fetch {
            id: 12345,
            subscriber_priority: 127,
            group_order: GroupOrder::Publisher,
            fetch_type: FetchType::RelativeJoining,
            track_namespace: Some(TrackNamespace::from_utf8_path("test/path/to/resource")),
            track_name: Some("audiotrack".to_string()),
            start_location: Some(Location::new(34, 53)),
            end_location: None,
            joining_id: None,
            joining_start: None,
            params: Default::default(),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));
    }
}

