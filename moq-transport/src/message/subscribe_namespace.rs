use crate::coding::{Decode, DecodeError, Encode, EncodeError, Params, TrackNamespace};

/// Subscribe Namespace
/// https://www.ietf.org/archive/id/draft-ietf-moq-transport-06.html#section-6.11
#[derive(Clone, Debug)]
pub struct SubscribeNamespace {
    /// The track namespace
    pub namespace_prefix: TrackNamespace,

    /// Optional parameters
    pub params: Params,
}

impl Decode for SubscribeNamespace {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let namespace_prefix = TrackNamespace::decode(r)?;
        let params = Params::decode(r)?;

        Ok(Self {
            namespace_prefix,
            params,
        })
    }
}

impl Encode for SubscribeNamespace {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.namespace_prefix.encode(w)?;
        self.params.encode(w)?;

        Ok(())
    }
}
