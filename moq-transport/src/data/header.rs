use std::fmt;
use crate::coding::{Decode, DecodeError, Encode, EncodeError};
use crate::data::{SubgroupHeader, FetchHeader};

/// Stream Header Types
#[repr(u64)]
#[derive(Copy, Debug, Clone, Eq, PartialEq)]
pub enum StreamHeaderType {
    SubgroupZeroId                = 0x10,
    SubgroupZeroIdExt             = 0x11,
    SubgroupOjbectId              = 0x12,
    SubgroupOjbectIdExt           = 0x13,
    SubgroupId                    = 0x14,
    SubgroupIdExt                 = 0x15,
    SubgroupZeroIdEndOfGroup      = 0x18,
    SubgroupZeroIdExtEndOfGroup   = 0x19,
    SubgroupObjectIdEndOfGroup    = 0x1a,
    SubgroupOjbectIdExtEndOfGroup = 0x1b,
    SubgroupIdEndOfGroup          = 0x1c,
    SubgroupIdExtEndOfGroup       = 0x1d,
    Fetch                         = 0x5,
}

impl StreamHeaderType {
    pub fn is_subgroup(&self) -> bool {
        let header_type = *self as u64;
        header_type >= 0x10 && header_type <= 0x1d
    }

    pub fn is_fetch(&self) -> bool {
        *self == StreamHeaderType::Fetch
    }

    pub fn has_extension_headers(&self) -> bool {
        match *self {
            StreamHeaderType::SubgroupZeroIdExt |
            StreamHeaderType::SubgroupOjbectIdExt |
            StreamHeaderType::SubgroupIdExt |
            StreamHeaderType::SubgroupZeroIdExtEndOfGroup |
            StreamHeaderType::SubgroupOjbectIdExtEndOfGroup |
            StreamHeaderType::SubgroupIdExtEndOfGroup |
            StreamHeaderType::Fetch => true,
            _ => false
        }
    }

    pub fn has_subgroup_id(&self) -> bool {
        match *self {
            StreamHeaderType::SubgroupId |
            StreamHeaderType::SubgroupIdExt |
            StreamHeaderType::SubgroupIdEndOfGroup |
            StreamHeaderType::SubgroupIdExtEndOfGroup => true,
            _ => false
        }
    }
}

impl Encode for StreamHeaderType {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        let val = *self as u64;
        val.encode(w)?;
        Ok(())
    }
}

impl Decode for StreamHeaderType {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let header_type = match u64::decode(r)? {
            0x10_u64 => Ok(Self::SubgroupZeroId),
            0x11_u64 => Ok(Self::SubgroupZeroIdExt),
            0x12_u64 => Ok(Self::SubgroupOjbectId),
            0x13_u64 => Ok(Self::SubgroupOjbectIdExt),
            0x14_u64 => Ok(Self::SubgroupId),
            0x15_u64 => Ok(Self::SubgroupIdExt),
            0x18_u64 => Ok(Self::SubgroupZeroIdEndOfGroup),
            0x19_u64 => Ok(Self::SubgroupZeroIdExtEndOfGroup),
            0x1a_u64 => Ok(Self::SubgroupObjectIdEndOfGroup),
            0x1b_u64 => Ok(Self::SubgroupOjbectIdExtEndOfGroup),
            0x1c_u64 => Ok(Self::SubgroupIdEndOfGroup),
            0x1d_u64 => Ok(Self::SubgroupIdExtEndOfGroup),
            0x05_u64 => Ok(Self::Fetch),
            _ => Err(DecodeError::InvalidHeaderType),
        };

        if let Ok(header_type_inner) = header_type {
            log::trace!("stream header type: {header_type_inner}");
        }

        header_type
    }
}

impl fmt::Display for StreamHeaderType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({:#x})", self, *self as u64)
    }
}


#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StreamHeader {
    /// Subgroup Header Type
    pub header_type: StreamHeaderType,

    /// Subgroup Header for StreamHeaderTypes that are Subgroup header types
    pub subgroup_header: Option<SubgroupHeader>,

    /// Fetch Header for StreamHeaderTypes that are Fetch header types
    pub fetch_header: Option<FetchHeader>,
}

impl Decode for StreamHeader {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let header_type = StreamHeaderType::decode(r)?;

        let subgroup_header = match header_type.is_subgroup() {
            true => Some(SubgroupHeader::decode(header_type, r)?),
            false => None,
        };

        let fetch_header = match header_type.is_fetch() {
            true => Some(FetchHeader::decode(header_type, r)?),
            false => None,
        };

        Ok(Self {
            header_type,
            subgroup_header,
            fetch_header,
        })
    }
}

impl Encode for StreamHeader {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        // Note: we are intentionally not encoding the header_type here, it will be encoded in the
        //       appropriate substructures.
        //self.header_type.encode(w)?;
        if self.header_type.is_subgroup() {
            if let Some(subgroup_header) = &self.subgroup_header {
                subgroup_header.encode(w)?;
            } else {
                return Err(EncodeError::MissingField("SubgroupHeader".to_string()));
            }
        } else {
            if let Some(fetch_header) = &self.fetch_header {
                fetch_header.encode(w)?;
            } else {
                return Err(EncodeError::MissingField("FetchHeader".to_string()));
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
    fn encode_decode_stream_header_type() {
        let mut buf = BytesMut::new();

        let ht = StreamHeaderType::Fetch;
        ht.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x05 ]);
        let decoded = StreamHeaderType::decode(&mut buf).unwrap();
        assert_eq!(decoded, ht);
        assert!(ht.is_fetch());
        assert!(!ht.is_subgroup());
        assert!(!ht.has_subgroup_id());

        let ht = StreamHeaderType::SubgroupZeroId;
        ht.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x10 ]);
        let decoded = StreamHeaderType::decode(&mut buf).unwrap();
        assert_eq!(decoded, ht);
        assert!(ht.is_subgroup());
        assert!(!ht.is_fetch());
        assert!(!ht.has_subgroup_id());
    }

    #[test]
    fn decode_bad_stream_header_type() {
        let data: Vec<u8> = vec![ 0x00 ];  // Invalid filter type
        let mut buf: Bytes = data.into();
        let result = StreamHeaderType::decode(&mut buf);
        assert!(matches!(result, Err(DecodeError::InvalidHeaderType)));
    }

    #[test]
    fn encode_decode_stream_header() {
        let mut buf = BytesMut::new();

        let sh = StreamHeader {
            header_type: StreamHeaderType::Fetch,
            subgroup_header: None,
            fetch_header: Some(FetchHeader { header_type: StreamHeaderType::Fetch, request_id: 10 }),
        };
        sh.encode(&mut buf).unwrap();
        let decoded = StreamHeader::decode(&mut buf).unwrap();
        assert_eq!(decoded, sh);
        assert!(sh.header_type.is_fetch());
        assert!(!sh.header_type.is_subgroup());
        assert!(!sh.header_type.has_subgroup_id());

        let sh = StreamHeader {
            header_type: StreamHeaderType::SubgroupId,
            subgroup_header: Some(SubgroupHeader { header_type: StreamHeaderType::SubgroupId, track_alias: 10, group_id: 0, subgroup_id: Some(1), publisher_priority: 100 }),
            fetch_header: None
        };
        sh.encode(&mut buf).unwrap();
        let decoded = StreamHeader::decode(&mut buf).unwrap();
        assert_eq!(decoded, sh);
        assert!(sh.header_type.is_subgroup());
        assert!(!sh.header_type.is_fetch());
        assert!(sh.header_type.has_subgroup_id());
}
}
