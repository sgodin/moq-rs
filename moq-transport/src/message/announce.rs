use crate::coding::{Decode, DecodeError, Encode, EncodeError, KeyValuePairs, TrackNamespace};

/// Sent by the publisher to announce the availability of a group of tracks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Announce {
    /// The request ID
    pub id: u64,

    /// The track namespace
    pub track_namespace: TrackNamespace,

    /// Optional parameters
    pub params: KeyValuePairs,
}

impl Decode for Announce {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let id = u64::decode(r)?;
        let track_namespace = TrackNamespace::decode(r)?;
        let params = KeyValuePairs::decode(r)?;

        Ok(Self { id, track_namespace, params })
    }
}

impl Encode for Announce {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.id.encode(w)?;
        self.track_namespace.encode(w)?;
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

        let msg = Announce {
            id: 12345,
            track_namespace: TrackNamespace::from_utf8_path("test/path/to/resource"),
            params: kvps.clone(),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = Announce::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }
}

