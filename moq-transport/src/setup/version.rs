use crate::coding::{Decode, DecodeError, Encode, EncodeError, VarInt};

use std::ops::Deref;
use std::fmt;

/// A version number negotiated during the setup.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(pub u32);

impl Version {
    // TODO SLG - eventually remove this version, since we won't ever parse it
    //            given that we only parse CLIENT_SETUP messages with id 0x20
    /// https://www.ietf.org/archive/id/draft-ietf-moq-transport-07.html
    pub const DRAFT_07: Version = Version(0xff000007);

    /// First version we might see in CLIENT_SETUP (0x20) or SERVER_SETUP (0x21)
    /// https://www.ietf.org/archive/id/draft-ietf-moq-transport-11.html
    pub const DRAFT_11: Version = Version(0xff00000b);

    /// https://www.ietf.org/archive/id/draft-ietf-moq-transport-12.html
    pub const DRAFT_12: Version = Version(0xff00000c);

    /// https://www.ietf.org/archive/id/draft-ietf-moq-transport-13.html
    pub const DRAFT_13: Version = Version(0xff00000d);
}

impl From<u32> for Version {
    fn from(v: u32) -> Self {
        Self(v)
    }
}

impl From<Version> for u32 {
    fn from(v: Version) -> Self {
        v.0
    }
}

impl Decode for Version {
    /// Decode the version number.
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let v = VarInt::decode(r)?;
        Ok(Self(u32::try_from(v).map_err(|e| DecodeError::BoundsExceeded(e))?))
    }
}

impl Encode for Version {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        VarInt::from_u32(self.0).encode(w)?;
        Ok(())
    }
}

impl fmt::Debug for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Just reuse the Display formatting
        write!(f, "{}", self)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 > 0xff000000 {
            write!(f, "DRAFT_{:02x}", self.0 & 0x00ffffff)
        } else {
            self.0.fmt(f)
        }
    }
}

/// A list of versions in arbitrary order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Versions(Vec<Version>);

impl Decode for Versions {
    /// Decode the version list.
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let count = u64::decode(r)?;
        let mut vs = Vec::new();

        for _ in 0..count {
            let v = Version::decode(r)?;
            vs.push(v);
        }

        Ok(Self(vs))
    }
}

impl Encode for Versions {
    /// Encode the version list.
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.0.len().encode(w)?;

        for v in &self.0 {
            v.encode(w)?;
        }

        Ok(())
    }
}

impl Deref for Versions {
    type Target = Vec<Version>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<Vec<Version>> for Versions {
    fn from(vs: Vec<Version>) -> Self {
        Self(vs)
    }
}

impl<const N: usize> From<[Version; N]> for Versions {
    fn from(vs: [Version; N]) -> Self {
        Self(vs.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn encode_decode() {
        let mut buf = BytesMut::new();
        let versions = Versions(vec![
            Version(1),
            Version::DRAFT_07,
            Version::DRAFT_13,
        ]);

        versions.encode(&mut buf).unwrap();
        assert_eq!(
            buf.to_vec(),
            vec![
                0x03,  // 3 Versions
                0x01,  // Version 1
                0xC0, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x07,   // Version DRAFT_07 (0xff000007)
                0xC0, 0x00, 0x00, 0x00, 0xFF, 0x00, 0x00, 0x0D,   // Version DRAFT_07 (0xff00000d)
            ]
        );

        let decoded = Versions::decode(&mut buf).unwrap();
        assert_eq!(decoded, versions);
    }
}
