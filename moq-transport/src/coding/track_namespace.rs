use super::{Decode, DecodeError, Encode, EncodeError, TupleField};
use core::hash::{Hash, Hasher};
use std::fmt;

/// TrackNamespace
#[derive(Clone, Default, Eq, PartialEq)]
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

    /// Check if this namespace is a prefix of another namespace.
    /// Returns true if all fields in `self` match the beginning fields of `other`.
    pub fn is_prefix_of(&self, other: &TrackNamespace) -> bool {
        if self.fields.len() > other.fields.len() {
            return false;
        }
        self.fields
            .iter()
            .zip(other.fields.iter())
            .all(|(a, b)| a == b)
    }

    /// Get all prefixes of this namespace, from longest to shortest (not including empty).
    /// For example, "a/b/c" returns ["a/b/c", "a/b", "a"].
    pub fn get_prefixes(&self) -> Vec<TrackNamespace> {
        let mut prefixes = Vec::new();
        for i in (1..=self.fields.len()).rev() {
            let mut prefix = TrackNamespace::new();
            prefix.fields = self.fields[0..i].to_vec();
            prefixes.push(prefix);
        }
        prefixes
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
            return Err(DecodeError::FieldBoundsExceeded(
                "TrackNamespace tuples".to_string(),
            ));
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
            return Err(EncodeError::FieldBoundsExceeded(
                "TrackNamespace tuples".to_string(),
            ));
        }
        self.fields.len().encode(w)?;
        for field in &self.fields {
            field.encode(w)?;
        }
        Ok(())
    }
}

impl fmt::Debug for TrackNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Just reuse the Display formatting
        write!(f, "{self}")
    }
}

impl fmt::Display for TrackNamespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{0}", self.to_utf8_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use bytes::BytesMut;

    #[test]
    fn encode_decode() {
        let mut buf = BytesMut::new();

        let t = TrackNamespace::from_utf8_path("test/path/to/resource");
        t.encode(&mut buf).unwrap();
        #[rustfmt::skip]
        assert_eq!(
            buf.to_vec(),
            vec![
                0x04, // 4 tuple fields
                // Field 1: "test"
                0x04, 0x74, 0x65, 0x73, 0x74,
                // Field 2: "path"
                0x04, 0x70, 0x61, 0x74, 0x68,
                // Field 3: "to"
                0x02, 0x74, 0x6f,
                // Field 4: "resource"
                0x08, 0x72, 0x65, 0x73, 0x6f, 0x75, 0x72, 0x63, 0x65
            ]
        );
        let decoded = TrackNamespace::decode(&mut buf).unwrap();
        assert_eq!(decoded, t);

        // Alternate construction
        let mut t = TrackNamespace::new();
        t.add(TupleField::from_utf8("test"));
        t.encode(&mut buf).unwrap();
        assert_eq!(
            buf.to_vec(),
            vec![
                0x01, // 1 tuple field
                // Field 1: "test"
                0x04, 0x74, 0x65, 0x73, 0x74
            ]
        );
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
        assert!(matches!(
            encoded.unwrap_err(),
            EncodeError::FieldBoundsExceeded(_)
        ));
    }

    #[test]
    fn decode_too_large() {
        let mut data: Vec<u8> = vec![0x00; 256]; // Create a vector with 256 bytes
        data[0] = (TrackNamespace::MAX_FIELDS + 1) as u8; // Set first byte (count) to 33 as a VarInt
        let mut buf: Bytes = data.into();
        let decoded = TrackNamespace::decode(&mut buf);
        assert!(matches!(
            decoded.unwrap_err(),
            DecodeError::FieldBoundsExceeded(_)
        ));
    }

    #[test]
    fn test_is_prefix_of() {
        let ns1 = TrackNamespace::from_utf8_path("moq-test-00");
        let ns2 = TrackNamespace::from_utf8_path("moq-test-00/1/2/3");
        let ns3 = TrackNamespace::from_utf8_path("moq-test-00/1");
        let ns4 = TrackNamespace::from_utf8_path("other");

        // Test prefix matching
        assert!(ns1.is_prefix_of(&ns2));
        assert!(ns1.is_prefix_of(&ns3));
        assert!(ns1.is_prefix_of(&ns1));
        assert!(ns3.is_prefix_of(&ns2));

        // Test non-matching
        assert!(!ns2.is_prefix_of(&ns1)); // Longer is not prefix of shorter
        assert!(!ns4.is_prefix_of(&ns1)); // Different namespace
        assert!(!ns4.is_prefix_of(&ns2));
    }

    #[test]
    fn test_get_prefixes() {
        let ns = TrackNamespace::from_utf8_path("moq-test-00/1/2/3");
        let prefixes = ns.get_prefixes();

        assert_eq!(prefixes.len(), 4);
        assert_eq!(prefixes[0].to_utf8_path(), "/moq-test-00/1/2/3");
        assert_eq!(prefixes[1].to_utf8_path(), "/moq-test-00/1/2");
        assert_eq!(prefixes[2].to_utf8_path(), "/moq-test-00/1");
        assert_eq!(prefixes[3].to_utf8_path(), "/moq-test-00");

        // Test single element
        let ns_single = TrackNamespace::from_utf8_path("moq-test-00");
        let prefixes_single = ns_single.get_prefixes();
        assert_eq!(prefixes_single.len(), 1);
        assert_eq!(prefixes_single[0].to_utf8_path(), "/moq-test-00");
    }
}
