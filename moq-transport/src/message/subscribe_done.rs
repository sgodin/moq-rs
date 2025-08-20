use crate::coding::{Decode, DecodeError, Encode, EncodeError, VarInt, ReasonPhrase};

/// Sent by the publisher to cleanly terminate a Subscription.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscribeDone {
    // The request ID of the subscription being terminated.
    pub id: u64,

    /// The status code indicating why the subscription ended.
    pub status_code: u64,

    /// The number of data streams the publisher opened for this subscription.
    pub stream_count: u64,

    /// Provides the reason for the subscription error.
    pub reason: ReasonPhrase,
}

impl Decode for SubscribeDone {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let id = VarInt::decode(r)?.into_inner();
        let status_code = VarInt::decode(r)?.into_inner();
        let stream_count = VarInt::decode(r)?.into_inner();
        let reason = ReasonPhrase::decode(r)?;

        Ok(Self {
            id,
            status_code,
            stream_count,
            reason,
        })
    }
}

impl Encode for SubscribeDone {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        VarInt::try_from(self.id)?.encode(w)?;
        VarInt::try_from(self.status_code)?.encode(w)?;
        VarInt::try_from(self.stream_count)?.encode(w)?;
        self.reason.encode(w)?;

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

        let msg = SubscribeDone {
            id: 12345,
            status_code: 0x02,
            stream_count: 2,
            reason: ReasonPhrase("Track Ended".to_string()),
        };
        msg.encode(&mut buf).unwrap();
        let decoded = SubscribeDone::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }
}
