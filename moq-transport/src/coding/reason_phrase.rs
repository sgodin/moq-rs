use super::{Decode, DecodeError, Encode, EncodeError};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReasonPhrase(pub String);

impl ReasonPhrase {
    /// The max phrase length.
    pub const MAX_LEN: usize = 1024;
}

impl Encode for ReasonPhrase {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        if self.0.len() > ReasonPhrase::MAX_LEN {
            return Err(EncodeError::FieldBoundsExceeded("ReasonPhrase".to_string()));
        }
        self.0.len().encode(w)?;
        Self::encode_remaining(w, self.0.len())?;
        w.put(self.0.as_ref());
        Ok(())
    }
}

impl Decode for ReasonPhrase {
    /// Decode a string with a varint length prefix.
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let size = usize::decode(r)?;
        if size > ReasonPhrase::MAX_LEN {
            return Err(DecodeError::FieldBoundsExceeded("ReasonPhrase".to_string()));
        }

        Self::decode_remaining(r, size)?;

        let mut buf = vec![0; size];
        r.copy_to_slice(&mut buf);
        let str = String::from_utf8(buf)?;

        Ok(ReasonPhrase(str))
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;
    use bytes::Bytes;

    #[test]
    fn encode_decode() {
        let mut buf = BytesMut::new();

        let r = ReasonPhrase("testreason".to_string());
        r.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![
            0x0a,  // Length of "testreason" is 10
            0x74, 0x65, 0x73, 0x74, 0x72, 0x65, 0x61, 0x73, 0x6f, 0x6e ]);
        let decoded = ReasonPhrase::decode(&mut buf).unwrap();
        assert_eq!(decoded, r);
    }

    #[test]
    fn encode_too_large() {
        let mut buf = BytesMut::new();

        let r = ReasonPhrase("x".repeat(1025));
        let encoded = r.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::FieldBoundsExceeded(_)));
    }

    #[test]
    fn decode_too_large() {
        let mut data: Vec<u8> = vec![ 0x00; 1025 ];  // Create a vector with 1025 bytes
        // Set first 2 bytes as length of 1025 as a VarInt
        data[0] = 0x44;
        data[1] = 0x01;
        let mut buf: Bytes = data.into();
        let decoded = ReasonPhrase::decode(&mut buf);
        assert!(matches!(decoded.unwrap_err(), DecodeError::FieldBoundsExceeded(_)));
    }
}
