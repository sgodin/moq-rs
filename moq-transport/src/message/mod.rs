//! Low-level message sent over the wire, as defined in the specification.
//!
//! TODO Update this
//! All of these messages are sent over a bidirectional QUIC stream.
//! This introduces some head-of-line blocking but preserves ordering.
//! The only exception are OBJECT "messages", which are sent over dedicated QUIC streams.
//!
//! Messages sent by the publisher:
//! - [Announce]
//! - [Unannounce]
//! - [SubscribeOk]
//! - [SubscribeError]
//! - [SubscribeReset]
//! - [Object]
//!
//! Messages sent by the subscriber:
//! - [Subscribe]
//! - [SubscribeUpdate]
//! - [Unsubscribe]
//! - [AnnounceOk]
//! - [AnnounceError]
//!
//! Example flow:
//! ```test
//!  -> ANNOUNCE        namespace="foo"
//!  <- ANNOUNCE_OK     namespace="foo"
//!  <- SUBSCRIBE       id=0 namespace="foo" name="bar"
//!  -> SUBSCRIBE_OK    id=0
//!  -> OBJECT          id=0 sequence=69 priority=4 expires=30
//!  -> OBJECT          id=0 sequence=70 priority=4 expires=30
//!  -> OBJECT          id=0 sequence=70 priority=4 expires=30
//!  <- SUBSCRIBE_STOP  id=0
//!  -> SUBSCRIBE_RESET id=0 code=206 reason="closed by peer"
//! ```
mod announce;
mod announce_cancel;
mod announce_error;
mod announce_ok;
mod fetch;
mod fetch_cancel;
mod fetch_error;
mod fetch_ok;
mod fetch_type;
mod filter_type;
mod go_away;
mod group_order;
mod max_request_id;
mod publish;
mod publish_ok;
mod publish_error;
mod publisher;
mod requests_blocked;
mod subscribe;
mod subscribe_done;
mod subscribe_error;
mod subscribe_namespace;
mod subscribe_namespace_error;
mod subscribe_namespace_ok;
mod subscribe_ok;
mod subscribe_update;
mod subscriber;
mod track_status;
mod track_status_ok;
mod track_status_error;
mod unannounce;
mod unsubscribe;
mod unsubscribe_namespace;

pub use announce::*;
pub use announce_cancel::*;
pub use announce_error::*;
pub use announce_ok::*;
pub use fetch::*;
pub use fetch_cancel::*;
pub use fetch_error::*;
pub use fetch_ok::*;
pub use fetch_type::*;
pub use filter_type::*;
pub use go_away::*;
pub use group_order::*;
pub use max_request_id::*;
pub use requests_blocked::*;
pub use publish::*;
pub use publish_ok::*;
pub use publish_error::*;
pub use publisher::*;
pub use subscribe::*;
pub use subscribe_done::*;
pub use subscribe_error::*;
pub use subscribe_namespace::*;
pub use subscribe_namespace_error::*;
pub use subscribe_namespace_ok::*;
pub use subscribe_ok::*;
pub use subscribe_update::*;
pub use subscriber::*;
pub use track_status::*;
pub use track_status_ok::*;
pub use track_status_error::*;
pub use unannounce::*;
pub use unsubscribe::*;
pub use unsubscribe_namespace::*;

use crate::coding::{Decode, DecodeError, Encode, EncodeError};
use std::fmt;

// Use a macro to generate the message types rather than copy-paste.
// This implements a decode/encode method that uses the specified type.
macro_rules! message_types {
    {$($name:ident = $val:expr,)*} => {
		/// All supported message types.
		#[derive(Clone)]
		pub enum Message {
			$($name($name)),*
		}

		impl Decode for Message {
			fn decode<R: bytes::Buf>(r: &mut R) -> Result<Self, DecodeError> {
				let t = u64::decode(r)?;
				let _len = u16::decode(r)?;

				// TODO: Check the length of the message.

				match t {
					$($val => {
						let msg = $name::decode(r)?;
						Ok(Self::$name(msg))
					})*
					_ => Err(DecodeError::InvalidMessage(t)),
				}
			}
		}

		impl Encode for Message {
			fn encode<W: bytes::BufMut>(&self, w: &mut W) -> Result<(), EncodeError> {
				match self {
					$(Self::$name(ref m) => {
						self.id().encode(w)?;

						// Find out the length of the message
						// by encoding it into a buffer and then encoding the length.
						// This is a bit wasteful, but it's the only way to know the length.
                        // TODO SLG - perhaps we can store the position of the Length field in the BufMut and
                        //       write the length later, to avoid the copy of the message bytes?
						let mut buf = Vec::new();
						m.encode(&mut buf).unwrap();
                        if buf.len() > u16::MAX as usize {
                            return Err(EncodeError::MsgBoundsExceeded);
                        }
                        (buf.len() as u16).encode(w)?;

						// At least don't encode the message twice.
						// Instead, write the buffer directly to the writer.
                        Self::encode_remaining(w, buf.len())?;
						w.put_slice(&buf);
						Ok(())
					},)*
				}
			}
		}

		impl Message {
			pub fn id(&self) -> u64 {
				match self {
					$(Self::$name(_) => {
						$val
					},)*
				}
			}

			pub fn name(&self) -> &'static str {
				match self {
					$(Self::$name(_) => {
						stringify!($name)
					},)*
				}
			}
		}

		$(impl From<$name> for Message {
			fn from(m: $name) -> Self {
				Message::$name(m)
			}
		})*

		impl fmt::Debug for Message {
			// Delegate to the message formatter
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				match self {
					$(Self::$name(ref m) => m.fmt(f),)*
				}
			}
		}
    }
}

// Each message is prefixed with the given VarInt type.
message_types! {
    // NOTE: Object and Setup are in other modules.
    // Object = 0x0
    // ObjectUnbounded = 0x2
    // SetupClient = 0x20
    // SetupServer = 0x21
    // SetupClient = 0x40  // legacy, used in draft versions <= 10
    // SetupServer = 0x41  // legacy, used in draft versions <= 10

    // SUBSCRIBE family, sent by subscriber
    SubscribeUpdate = 0x2,
    Subscribe = 0x3,
    Unsubscribe = 0xa,

    // SUBSCRIBE family, sent by publisher
    SubscribeOk = 0x4,
    SubscribeError = 0x5,
    SubscribeDone = 0xb,
    MaxRequestId = 0x15,

    // ANNOUNCE family, sent by publisher
    Announce = 0x6,
    Unannounce = 0x9,

    // ANNOUNCE family, sent by subscriber
    AnnounceOk = 0x7,
    AnnounceError = 0x8,
    AnnounceCancel = 0xc,

    // TRACK_STATUS, sent by subscriber
    TrackStatus = 0xd,
    TrackStatusOk = 0xe,
    TrackStatusError = 0xf,

    // Misc
    GoAway = 0x10,

    // NAMESPACE family, sent by subscriber
    SubscribeNamespace = 0x11,
    SubscribeNamespaceOk = 0x12,
    SubscribeNamespaceError = 0x13,
    UnsubscribeNamespace = 0x14,

    // FETCH family, sent by subscriber
    Fetch = 0x16,
    FetchCancel = 0x17,
    FetchOk = 0x18,
    FetchError = 0x19,

    RequestsBlocked = 0x1a,

    Publish = 0x1d,
    PublishOk = 0x1e,
    PublishError = 0x1f,
}
