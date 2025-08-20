use super::{Decode, DecodeError, Encode, EncodeError, TupleField};
use core::hash::{Hash, Hasher};

/// TrackNamespace
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TrackNamespace {
    pub fields: Vec<TupleField>,
}

impl TrackNamespace {
    pub const MAX_FIELDS: usize = 32;

    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, field: TupleField) {
        self.fields.push(field);
    }

    pub fn clear(&mut self) {
        self.fields.clear();
    }

    pub fn from_utf8_path(path: &str) -> Self {
        let mut tuple = TrackNamespace::new();
        for part in path.split('/') {
            tuple.add(TupleField::from_utf8(part));
        }
        tuple
    }

    pub fn to_utf8_path(&self) -> String {
        let mut path = String::new();
        for field in &self.fields {
            path.push('/');
            path.push_str(&String::from_utf8_lossy(&field.value));
        }
        path
    }
}

impl Hash for TrackNamespace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.fields.hash(state);
    }
}

impl Decode for TrackNamespace {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let count = usize::decode(r)?;
        if count > Self::MAX_FIELDS {
            return Err(DecodeError::FieldBoundsExceeded("TrackNamespace tuples".to_string()));
        }

        let mut fields = Vec::new();
        for _ in 0..count {
            fields.push(TupleField::decode(r)?);
        }
        Ok(Self { fields })
    }
}

impl Encode for TrackNamespace {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        if self.fields.len() > Self::MAX_FIELDS {
            return Err(EncodeError::FieldBoundsExceeded("TrackNamespace tuples".to_string()));
        }
        self.fields.len().encode(w)?;
        for field in &self.fields {
            field.encode(w)?;
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
    fn encode_decode() {
        let mut buf = BytesMut::new();

        let t = TrackNamespace::from_utf8_path("test/path/to/resource");
        t.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![
            0x04,  // 4 tuple fields
            0x04, 0x74, 0x65, 0x73, 0x74, // Field 1: "test"
            0x04, 0x70, 0x61, 0x74, 0x68, // Field 2: "path"
            0x02, 0x74, 0x6f, // Field 3: "to"
            0x08, 0x72, 0x65, 0x73, 0x6f, 0x75, 0x72, 0x63, 0x65]); // Field 4: "resource"
        let decoded = TrackNamespace::decode(&mut buf).unwrap();
        assert_eq!(decoded, t);

        // Alternate construction
        let mut t = TrackNamespace::new();
        t.add(TupleField::from_utf8("test"));
        t.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![
            0x01,  // 1 tuple field
            0x04, 0x74, 0x65, 0x73, 0x74 ]); // Field 1: "test"
        let decoded = TrackNamespace::decode(&mut buf).unwrap();
        assert_eq!(decoded, t);
    }

    #[test]
    fn encode_too_large() {
        let mut buf = BytesMut::new();

        let mut t = TrackNamespace::new();
        for i in 0..TrackNamespace::MAX_FIELDS + 1 {
            t.add(TupleField::from_utf8(&format!("field{}", i)));
        }

        let encoded = t.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::FieldBoundsExceeded(_)));
    }

    #[test]
    fn decode_too_large() {
        let mut data: Vec<u8> = vec![ 0x00; 256 ];  // Create a vector with 256 bytes
        data[0] = (TrackNamespace::MAX_FIELDS + 1) as u8; // Set first byte (count) to 33 as a VarInt
        let mut buf: Bytes = data.into();
        let decoded = TrackNamespace::decode(&mut buf);
        assert!(matches!(decoded.unwrap_err(), DecodeError::FieldBoundsExceeded(_)));
    }
}
