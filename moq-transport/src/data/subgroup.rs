use crate::coding::{Decode, DecodeError, Encode, EncodeError, KeyValuePairs};
use crate::data::ObjectStatus;

#[derive(Clone, Debug)]
pub struct SubgroupHeader {
    /// Subgroup Header Type - definition in flux - TODO eventually make into an enum
    pub header_type: u64,

    /// The track alias.
    pub track_alias: u64,

    /// The group sequence number
    pub group_id: u64,

    /// The subgroup sequence number
    pub subgroup_id: Option<u64>,

    /// Publisher priority, where **smaller** values are sent first.
    pub publisher_priority: u8,
}

impl Decode for SubgroupHeader {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let header_type = u64::decode(r)?;
        let track_alias = u64::decode(r)?;
        let group_id = u64::decode(r)?;
        let subgroup_id = match header_type {
            b if b & (1 << 2) != 0 => Some(u64::decode(r)?), // Check 3rd-bit to see if SubGroupId is present
            _ => None,
        };
        let publisher_priority = u8::decode(r)?;

        Ok(Self {
            header_type,
            track_alias,
            group_id,
            subgroup_id,
            publisher_priority,
        })
    }
}

impl Encode for SubgroupHeader {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.header_type.encode(w)?;
        self.track_alias.encode(w)?;
        self.group_id.encode(w)?;
        if self.header_type & (1 << 2) != 0 {
            if let Some(subgroup_id) = self.subgroup_id {
                subgroup_id.encode(w)?;
            } else {
                return Err(EncodeError::MissingField("SubgroupId".to_string()));
            }
        }
        self.publisher_priority.encode(w)?;

        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct SubgroupObject {
    pub object_id_delta: u64,
    pub extension_headers: Option<KeyValuePairs>,
    pub payload_length: usize,
    pub status: Option<ObjectStatus>,
    //pub payload: bytes::Bytes,  // TODO SLG - payload is sent outside this right now - decide which way to go
}

impl Decode for SubgroupObject {
    fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
        let object_id_delta = u64::decode(r)?;
        // TODO SLG - assume no extension headers for now, we currently don't know if they will be present or not without analysing the stream header type
        let extension_headers = None;
        let payload_length = usize::decode(r)?;
        let status = match payload_length {
            0 => Some(ObjectStatus::decode(r)?),
            _ => None,
        };

        //Self::decode_remaining(r, payload_length);
        //let payload = r.copy_to_bytes(payload_length);

        Ok(Self {
            object_id_delta,
            extension_headers,
            payload_length,
            status,
            //payload,
        })
    }
}

impl Encode for SubgroupObject {
    fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
        self.object_id_delta.encode(w)?;
        if let Some(extension_headers) = &self.extension_headers {
            extension_headers.encode(w)?;
        }
        self.payload_length.encode(w)?;
        if self.payload_length == 0 {
            if let Some(status) = self.status {
                status.encode(w)?;
            } else {
                return Err(EncodeError::MissingField("Status".to_string()));
            }
        }
        //Self::encode_remaining(w, self.payload.len())?;
        //w.put_slice(&self.payload);

        Ok(())
    }
}

// TODO SLG - add unit tests
