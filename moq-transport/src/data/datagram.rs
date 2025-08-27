use crate::{coding::{Decode, DecodeError, Encode, EncodeError, KeyValuePairs}};
use crate::data::ObjectStatus;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DatagramType {
    NoEndOfGroupNoExtensions   = 0x0,
    NoEndOfGroupWithExtensions = 0x1,
    EndOfGroupNoExtensions     = 0x2,
    EndOfGroupWithExtensions   = 0x3,
    StatusNoExtensions         = 0x4,
    StatusWithExtensions       = 0x5,
}

impl Decode for DatagramType {
    fn decode<B: bytes::Buf>(r: &mut B) -> Result<Self, DecodeError> {
        match u64::decode(r)? {
            0x0 => Ok(Self::NoEndOfGroupNoExtensions),
            0x1 => Ok(Self::NoEndOfGroupWithExtensions),
            0x2 => Ok(Self::EndOfGroupNoExtensions),
            0x3 => Ok(Self::EndOfGroupWithExtensions),
            0x4 => Ok(Self::StatusNoExtensions),
            0x5 => Ok(Self::StatusWithExtensions),
            _ => Err(DecodeError::InvalidDatagramType),
        }
    }
}

impl Encode for DatagramType {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        let val = *self as u64;
        val.encode(w)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Datagram {
    /// The type of this datagram object
    pub datagram_type: DatagramType,

    /// The track alias.
    pub track_alias: u64,

    /// The sequence number within the track.
    pub group_id: u64,

    /// The object ID within the group.
    pub object_id: u64,

    /// Publisher priority, where **smaller** values are sent first.
    pub publisher_priority: u8,

    /// Optional extension headers if type is 0x1 (NoEndOfGroupWithExtensions) or 0x3 (EndofGroupWithExtensions)
    pub extension_headers: Option<KeyValuePairs>,

    /// The Object Status.
    pub status: Option<ObjectStatus>,

    /// The payload.
    pub payload: Option<bytes::Bytes>,
}

impl Decode for Datagram {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let datagram_type = DatagramType::decode(r)?;
        let track_alias = u64::decode(r)?;
        let group_id = u64::decode(r)?;
        let object_id = u64::decode(r)?;
        let publisher_priority = u8::decode(r)?;
        let extension_headers = match datagram_type {
            DatagramType::NoEndOfGroupWithExtensions |
            DatagramType::EndOfGroupWithExtensions |
            DatagramType::StatusWithExtensions => Some(KeyValuePairs::decode(r)?),
            _ => None,
        };
        let status: Option<ObjectStatus>;
        let payload: Option<bytes::Bytes>;
        match datagram_type {
            DatagramType::StatusNoExtensions | DatagramType::StatusWithExtensions => {
                status = Some(ObjectStatus::decode(r)?);
                payload = None;
            }
            _ => {
                status = None;
                payload = Some(r.copy_to_bytes(r.remaining()));
            }
        };

        Ok(Self {
            datagram_type,
            track_alias,
            group_id,
            object_id,
            publisher_priority,
            extension_headers,
            status,
            payload,
        })
    }
}

impl Encode for Datagram {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.datagram_type.encode(w)?;
        self.track_alias.encode(w)?;
        self.group_id.encode(w)?;
        self.object_id.encode(w)?;
        self.publisher_priority.encode(w)?;
        match self.datagram_type {
            DatagramType::NoEndOfGroupWithExtensions |
            DatagramType::EndOfGroupWithExtensions |
            DatagramType::StatusWithExtensions => {
                if let Some(extension_headers) = &self.extension_headers {
                    extension_headers.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
            }
            _ => {}
        };
        match self.datagram_type {
            DatagramType::StatusNoExtensions | DatagramType::StatusWithExtensions => {
                if let Some(status) = &self.status {
                    status.encode(w)?;
                } else {
                    return Err(EncodeError::MissingField);
                }
            }
            _ => {
                if let Some(payload) = &self.payload {
                    Self::encode_remaining(w, payload.len())?;
                    w.put_slice(&payload);
                } else {
                    return Err(EncodeError::MissingField);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use bytes::Bytes;

    #[test]
    fn encode_decode_datagram_type() {
        let mut buf = BytesMut::new();

        let dt = DatagramType::NoEndOfGroupNoExtensions;
        dt.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x00 ]);
        let decoded = DatagramType::decode(&mut buf).unwrap();
        assert_eq!(decoded, dt);

        let dt = DatagramType::NoEndOfGroupWithExtensions;
        dt.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x01 ]);
        let decoded = DatagramType::decode(&mut buf).unwrap();
        assert_eq!(decoded, dt);

        let dt = DatagramType::EndOfGroupNoExtensions;
        dt.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x02 ]);
        let decoded = DatagramType::decode(&mut buf).unwrap();
        assert_eq!(decoded, dt);

        let dt = DatagramType::EndOfGroupWithExtensions;
        dt.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x03 ]);
        let decoded = DatagramType::decode(&mut buf).unwrap();
        assert_eq!(decoded, dt);

        let dt = DatagramType::StatusNoExtensions;
        dt.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x04 ]);
        let decoded = DatagramType::decode(&mut buf).unwrap();
        assert_eq!(decoded, dt);

        let dt = DatagramType::StatusWithExtensions;
        dt.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x05 ]);
        let decoded = DatagramType::decode(&mut buf).unwrap();
        assert_eq!(decoded, dt);
    }

    #[test]
    fn encode_decode_datagram() {
        let mut buf = BytesMut::new();

        // One ExtensionHeader for testing
        let mut kvps = KeyValuePairs::new();
        kvps.set_bytesvalue(123, vec![0x00, 0x01, 0x02, 0x03]);

        // DatagramType = NoEndOfGroupNoExtensions
        let msg = Datagram {
            datagram_type: DatagramType::NoEndOfGroupNoExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: None,
            payload: Some(Bytes::from("payload")),
        };
        msg.encode(&mut buf).unwrap();
        // Length should be: Type(1)+Alias(1)+GroupId(1)+ObjectId(2)+Priority(1)+Payload(7) = 13
        assert_eq!(13, buf.len());
        let decoded = Datagram::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // DatagramType = NoEndOfGroupWithExtensions
        let msg = Datagram {
            datagram_type: DatagramType::NoEndOfGroupWithExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: Some(kvps.clone()),
            status: None,
            payload: Some(Bytes::from("payload")),
        };
        msg.encode(&mut buf).unwrap();
        // Length should be: Same as above plus NumExt(1),ExtensionKey(2),ExtensionValueLen(1),ExtensionValue(4) = 13 + 8 = 21
        assert_eq!(21, buf.len());
        let decoded = Datagram::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // DatagramType = EndOfGroupNoExtensions
        let msg = Datagram {
            datagram_type: DatagramType::EndOfGroupNoExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: None,
            payload: Some(Bytes::from("payload")),
        };
        msg.encode(&mut buf).unwrap();
        // Length should be: Type(1)+Alias(1)+GroupId(1)+ObjectId(2)+Priority(1)+Payload(7) = 13
        assert_eq!(13, buf.len());
        let decoded = Datagram::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // DatagramType = EndOfGroupWithExtensions
        let msg = Datagram {
            datagram_type: DatagramType::EndOfGroupWithExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: Some(kvps.clone()),
            status: None,
            payload: Some(Bytes::from("payload")),
        };
        msg.encode(&mut buf).unwrap();
        // Length should be: Same as above plus NumExt(1),ExtensionKey(2),ExtensionValueLen(1),ExtensionValue(4) = 13 + 8 = 21
        assert_eq!(21, buf.len());
        let decoded = Datagram::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // DatagramType = StatusNoExtensions
        let msg = Datagram {
            datagram_type: DatagramType::StatusNoExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: Some(ObjectStatus::EndOfTrack),
            payload: None,
        };
        msg.encode(&mut buf).unwrap();
        // Length should be: Type(1)+Alias(1)+GroupId(1)+ObjectId(2)+Priority(1)+Status(1) = 7
        assert_eq!(7, buf.len());
        let decoded = Datagram::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);

        // DatagramType = StatusWithExtensions
        let msg = Datagram {
            datagram_type: DatagramType::StatusWithExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: Some(kvps.clone()),
            status: Some(ObjectStatus::EndOfTrack),
            payload: None,
        };
        msg.encode(&mut buf).unwrap();
        // Length should be: Same as above plus NumExt(1),ExtensionKey(2),ExtensionValueLen(1),ExtensionValue(4) = 7 + 8 = 15
        assert_eq!(15, buf.len());
        let decoded = Datagram::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }

    #[test]
    fn encode_datagram_missing_fields() {
        let mut buf = BytesMut::new();

        // DatagramType = NoEndOfGroupWithExtensions - missing extensions
        let msg = Datagram {
            datagram_type: DatagramType::NoEndOfGroupWithExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: None,
            payload: Some(Bytes::from("payload")),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // DatagramType = EndOfGroupWithExtensions - missing extensions
        let msg = Datagram {
            datagram_type: DatagramType::EndOfGroupWithExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: None,
            payload: Some(Bytes::from("payload")),
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // DatagramType = StatusWithExtensions - missing extensions
        let msg = Datagram {
            datagram_type: DatagramType::StatusWithExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: Some(ObjectStatus::EndOfTrack),
            payload: None,
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // DatagramType = NoEndOfGroupNoExtensions - missing payload
        let msg = Datagram {
            datagram_type: DatagramType::NoEndOfGroupNoExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: None,
            payload: None,
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));

        // DatagramType = StatusNoExtensions - missing status
        let msg = Datagram {
            datagram_type: DatagramType::StatusNoExtensions,
            track_alias: 12,
            group_id: 10,
            object_id: 1234,
            publisher_priority: 127,
            extension_headers: None,
            status: None,
            payload: None,
        };
        let encoded = msg.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::MissingField));
    }
}

