use super::{Decode, DecodeError, Encode, EncodeError};

impl Encode for u8 {
    /// Encode a u8 to the given writer.
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        let x = *self;
        w.put_u8(x);
        Ok(())
    }
}

impl Decode for u8 {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        Self::decode_remaining(r, 1)?;
        Ok(r.get_u8())
    }
}

impl Encode for u16 {
    /// Encode a u16 to the given writer.
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        let x = *self;
        w.put_u16(x);
        Ok(())
    }
}

impl Decode for u16 {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        Self::decode_remaining(r, 1)?;
        Ok(r.get_u16())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::BytesMut;

    #[test]
    fn encode_decode_u8() {
        let mut buf = BytesMut::new();

        let i: u8 = 8;
        i.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0x08 ]);
        let decoded = u8::decode(&mut buf).unwrap();
        assert_eq!(decoded, i);
    }

    #[test]
    fn encode_decode_u16() {
        let mut buf = BytesMut::new();

        let i: u16 = 65534;
        i.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![ 0xff, 0xfe ]);
        let decoded = u16::decode(&mut buf).unwrap();
        assert_eq!(decoded, i);
    }
}
