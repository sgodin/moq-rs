//! Messages used for the MoQ Transport handshake.
//!
//! After establishing the WebTransport session, the client creates a bidirectional QUIC stream.
//! The client sends the [Client] message and the server responds with the [Server] message.
//! Both sides negotate the [Version] and [Role].

mod client;
mod server;
mod version;
mod param_types;

pub use client::*;
pub use server::*;
pub use version::*;
pub use param_types::*;

pub const ALPN: &[u8] = b"moq-00";
