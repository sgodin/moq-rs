use crate::coding::{Decode, DecodeError, Encode, EncodeError, TrackNamespace};

/// Sent by the publisher to terminate an Announce.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unannounce {
    pub track_namespace: TrackNamespace,
}

impl Decode for Unannounce {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let track_namespace = TrackNamespace::decode(r)?;

        Ok(Self { track_namespace })
    }
}

impl Encode for Unannounce {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.track_namespace.encode(w)?;

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

        let msg = Unannounce {
            track_namespace: TrackNamespace::from_utf8_path("test/path/to/resource"),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = Unannounce::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }
}


