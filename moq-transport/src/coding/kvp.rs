use crate::coding::{Decode, DecodeError, Encode, EncodeError};
use bytes::Buf;
use std::collections::HashMap;
use std::fmt;

#[derive(Clone, Eq, PartialEq)]
pub enum Value {
    IntValue(u64),
    BytesValue(Vec<u8>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::IntValue(v) => write!(f, "{}", v),
            Value::BytesValue(bytes) => {
                // Show up to 16 bytes in hex for readability
                let preview: Vec<String> = bytes
                    .iter()
                    .take(16)
                    .map(|b| format!("{:02X}", b))
                    .collect();
                write!(f, "[{}]", preview.join(" "))
            }
        }
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct KeyValuePair {
    pub key: u64,
    pub value: Value,
}

impl KeyValuePair {
    pub fn new(key: u64, value: Value) -> Self {
        Self { key, value }
    }

    pub fn new_int(key: u64, value: u64) -> Self {
        Self {
            key,
            value: Value::IntValue(value),
        }
    }

    pub fn new_bytes(key: u64, value: Vec<u8>) -> Self {
        Self {
            key,
            value: Value::BytesValue(value),
        }
    }
}

impl Decode for KeyValuePair {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let key = u64::decode(r)?;

        if key % 2 == 0 {
            // VarInt variant
            let value = u64::decode(r)?;
            log::trace!("[KVP] Decoded even key={}, value={}", key, value);
            Ok(KeyValuePair::new_int(key, value))
        } else {
            // Bytes variant
            let length = usize::decode(r)?;
            log::trace!("[KVP] Decoded odd key={}, length={}", key, length);
            if length > u16::MAX as usize {
                log::error!(
                    "[KVP] Length exceeded! key={}, length={} (max={})",
                    key,
                    length,
                    u16::MAX
                );
                return Err(DecodeError::KeyValuePairLengthExceeded());
            }

            Self::decode_remaining(r, length)?;
            let mut buf = vec![0; length];
            r.copy_to_slice(&mut buf);
            Ok(KeyValuePair::new_bytes(key, buf))
        }
    }
}

impl Encode for KeyValuePair {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        match &self.value {
            Value::IntValue(v) => {
                // key must be even for IntValue
                if !self.key.is_multiple_of(2) {
                    return Err(EncodeError::InvalidValue);
                }
                self.key.encode(w)?;
                (*v).encode(w)?;
                Ok(())
            }
            Value::BytesValue(v) => {
                // key must be odd for BytesValue
                if self.key.is_multiple_of(2) {
                    return Err(EncodeError::InvalidValue);
                }
                self.key.encode(w)?;
                v.len().encode(w)?;
                Self::encode_remaining(w, v.len())?;
                w.put_slice(v);
                Ok(())
            }
        }
    }
}

impl fmt::Debug for KeyValuePair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{{}: {:?}}}", self.key, self.value)
    }
}

#[derive(Default, Clone, Eq, PartialEq)]
pub struct KeyValuePairs(pub HashMap<u64, KeyValuePair>);

impl KeyValuePairs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, kvp: KeyValuePair) {
        self.0.insert(kvp.key, kvp);
    }

    pub fn set_intvalue(&mut self, key: u64, value: u64) {
        self.0.insert(key, KeyValuePair::new_int(key, value));
    }

    pub fn set_bytesvalue(&mut self, key: u64, value: Vec<u8>) {
        self.0.insert(key, KeyValuePair::new_bytes(key, value));
    }

    pub fn has(&self, key: u64) -> bool {
        self.0.contains_key(&key)
    }

    pub fn get(&mut self, key: u64) -> Option<&KeyValuePair> {
        self.0.get(&key)
    }
}

impl Decode for KeyValuePairs {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        // Read total byte length of the encoded kvps
        let length = usize::decode(r)?;

        // Ensure we have that many bytes available in the input
        Self::decode_remaining(r, length)?;

        // If zero length, return empty map
        if length == 0 {
            return Ok(KeyValuePairs::new());
        }

        // Copy the exact slice that contains the encoded kvps and decode from it
        let mut buf = vec![0u8; length];
        r.copy_to_slice(&mut buf);
        let mut kvps_bytes = bytes::Bytes::from(buf);

        let mut kvps = HashMap::new();
        while kvps_bytes.has_remaining() {
            let kvp = KeyValuePair::decode(&mut kvps_bytes)?;
            if kvps.contains_key(&kvp.key) {
                return Err(DecodeError::DuplicateParameter(kvp.key));
            }
            kvps.insert(kvp.key, kvp);
        }

        Ok(KeyValuePairs(kvps))
    }
}

impl Encode for KeyValuePairs {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        // Encode all KeyValuePair entries into a temporary buffer to compute total byte length
        let mut tmp = bytes::BytesMut::new();
        for kvp in self.0.values() {
            kvp.encode(&mut tmp)?;
        }

        // Write total byte length (u64) followed by the encoded bytes
        (tmp.len() as u64).encode(w)?;
        w.put_slice(&tmp);

        Ok(())
    }
}

impl fmt::Debug for KeyValuePairs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{{ ")?;
        let pairs: Vec<_> = self.0.iter().collect();
        for (i, (_key, kv)) in pairs.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{:?}", kv)?;
        }
        write!(f, " }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use bytes::BytesMut;

    #[test]
    fn encode_decode_keyvaluepair() {
        let mut buf = BytesMut::new();

        // Type=1, VarInt value=0 - illegal with odd key/type
        let kvp = KeyValuePair::new(1, Value::IntValue(0));
        let encoded = kvp.encode(&mut buf);
        assert!(matches!(encoded.unwrap_err(), EncodeError::InvalidValue));

        // Type=0, VarInt value=0
        let kvp = KeyValuePair::new(0, Value::IntValue(0));
        kvp.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![0x00, 0x00]);
        let decoded = KeyValuePair::decode(&mut buf).unwrap();
        assert_eq!(decoded, kvp);

        // Type=100, VarInt value=100
        let kvp = KeyValuePair::new(100, Value::IntValue(100));
        kvp.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![0x40, 0x64, 0x40, 0x64]); // 2 2-byte VarInts with first 2 bits as 01
        let decoded = KeyValuePair::decode(&mut buf).unwrap();
        assert_eq!(decoded, kvp);

        // Type=0, Bytes value=[1,2,3,4,5] - illegal with even key/type
        let kvp = KeyValuePair::new(0, Value::BytesValue(vec![0x01, 0x02, 0x03, 0x04, 0x05]));
        let decoded = kvp.encode(&mut buf);
        assert!(matches!(decoded.unwrap_err(), EncodeError::InvalidValue));

        // Type=1, Bytes value=[1,2,3,4,5]
        let kvp = KeyValuePair::new(1, Value::BytesValue(vec![0x01, 0x02, 0x03, 0x04, 0x05]));
        kvp.encode(&mut buf).unwrap();
        assert_eq!(buf.to_vec(), vec![0x01, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05]);
        let decoded = KeyValuePair::decode(&mut buf).unwrap();
        assert_eq!(decoded, kvp);
    }

    #[test]
    fn decode_badtype() {
        // Simulate a VarInt value of 5, but with an odd key/type
        let data: Vec<u8> = vec![0x01, 0x05];
        let mut buf: Bytes = data.into();
        let decoded = KeyValuePair::decode(&mut buf);
        assert!(matches!(decoded.unwrap_err(), DecodeError::More(_))); // Framing will be off now
    }

    #[test]
    fn encode_decode_keyvaluepairs() {
        let mut buf = BytesMut::new();

        let mut kvps = KeyValuePairs::new();
        kvps.set_bytesvalue(1, vec![0x01, 0x02, 0x03, 0x04, 0x05]);
        kvps.encode(&mut buf).unwrap();
        assert_eq!(
            buf.to_vec(),
            vec![
                0x07, // 7 bytes total length
                0x01, 0x05, 0x01, 0x02, 0x03, 0x04, 0x05, // Key=1, Value=[1,2,3,4,5]
            ]
        );
        let decoded = KeyValuePairs::decode(&mut buf).unwrap();
        assert_eq!(decoded, kvps);

        let mut kvps = KeyValuePairs::new();
        kvps.set_intvalue(0, 0); // 2 bytes
        kvps.set_intvalue(100, 100); // 4 bytes
        kvps.set_bytesvalue(1, vec![0x01, 0x02, 0x03, 0x04, 0x05]); // 1 byte key, 1 byte length, 5 bytes data = 7 bytes
        kvps.encode(&mut buf).unwrap();
        let buf_vec = buf.to_vec();
        // Note:  Since KeyValuePairs is a HashMap, the order of KeyValuePairs in
        //        the encoded buffer is not guaranteed, so we can't validate the entire buffer,
        //        just validate the encoded length and the KeyValuePair length.
        assert_eq!(14, buf_vec.len()); // 14 bytes total (length + 3 kvps)
        assert_eq!(13, buf_vec[0]); // 13 bytes for the 3 KeyValuePairs data
        let decoded = KeyValuePairs::decode(&mut buf).unwrap();
        assert_eq!(decoded, kvps);
    }
}
