use crate::coding::{Decode, DecodeError, Encode, EncodeError, TrackNamespace};

#[derive(Clone, Debug)]
pub struct TrackStatusRequest {
    /// Track Namespace
    pub track_namespace: TrackNamespace,
    /// Track Name
    pub track_name: String,
}

impl Decode for TrackStatusRequest {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let track_namespace = TrackNamespace::decode(r)?;
        let track_name = String::decode(r)?;

        Ok(Self {
            track_namespace,
            track_name,
        })
    }
}

impl Encode for TrackStatusRequest {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.track_namespace.encode(w)?;
        self.track_name.encode(w)?;

        Ok(())
    }
}
