use crate::coding::{Decode, DecodeError, Encode, EncodeError, TrackNamespace, ReasonPhrase};

/// Sent by the subscriber to reject an Announce after ANNOUNCE_OK
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnounceCancel {
    // Echo back the namespace that was reset
    pub track_namespace: TrackNamespace,
    // An error code.
    pub error_code: u64,
    // An optional, human-readable reason.
    pub reason_phrase: ReasonPhrase,
}

impl Decode for AnnounceCancel {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let track_namespace = TrackNamespace::decode(r)?;
        let error_code = u64::decode(r)?;
        let reason_phrase = ReasonPhrase::decode(r)?;

        Ok(Self {
            track_namespace,
            error_code,
            reason_phrase,
        })
    }
}

impl Encode for AnnounceCancel {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.track_namespace.encode(w)?;
        self.error_code.encode(w)?;
        self.reason_phrase.encode(w)?;

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

        let msg = AnnounceCancel {
            track_namespace: TrackNamespace::from_utf8_path("testpath/video"),
            error_code: 0x2,
            reason_phrase: ReasonPhrase("Timeout".to_string()),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = AnnounceCancel::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }
}
