use crate::coding::{Decode, DecodeError, Encode, EncodeError};

/// Sent by the subscriber to accept an Announce.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnnounceOk {
    /// The request ID of the ANNOUNCE this message is replying to.
    pub id: u64,
}

impl Decode for AnnounceOk {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let id = u64::decode(r)?;
        Ok(Self { id })
    }
}

impl Encode for AnnounceOk {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.id.encode(w)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn encode_decode() {
        let mut buf = BytesMut::new();

        let msg = AnnounceOk {
            id: 12345,
        };
        msg.encode(&mut buf).unwrap();
        let decoded = AnnounceOk::decode(&mut buf).unwrap();
        assert_eq!(decoded, msg);
    }
}
