mod announce;
mod announced;
mod error;
mod publisher;
mod reader;
mod subscribe;
mod subscribed;
mod subscriber;
mod track_status_requested;
mod writer;

pub use announce::*;
pub use announced::*;
pub use error::*;
pub use publisher::*;
pub use subscribe::*;
pub use subscribed::*;
pub use subscriber::*;
pub use track_status_requested::*;

use reader::*;
use writer::*;

use futures::{stream::FuturesUnordered, StreamExt};
use std::sync::{atomic, Arc};

use crate::message::Message;
use crate::watch::Queue;
use crate::{message, setup};

/// Session object for managing all communications in a single QUIC connection.
#[must_use = "run() must be called"]
pub struct Session {
    webtransport: web_transport::Session,

    /// Control Stream Reader and Writer (QUIC bi-directional stream)
    sender: Writer, // Control Stream Sender
    recver: Reader, // Control Stream Receiver

    publisher: Option<Publisher>,   // Contains Publisher side logic, uses outgoing message queue to send control messages
    subscriber: Option<Subscriber>, // Contains Subscriber side logic, uses outgoing message queue to send control messages

    /// Queue used by Publisher and Subscriber for sending Control Messages
    outgoing: Queue<Message>,
}

impl Session {
    // Helper for determining the largest supported version
    fn largest_common<T: Ord + Clone + Eq>(a: &[T], b: &[T]) -> Option<T> {
        a.iter()
            .filter(|x| b.contains(x)) // keep only items also in b
            .cloned()                  // clone because we return T, not &T
            .max()                     // take the largest
    }

    fn new(
        webtransport: web_transport::Session,
        sender: Writer,
        recver: Reader,
        first_requestid: u64,
    ) -> (Self, Option<Publisher>, Option<Subscriber>) {
        let next_requestid = Arc::new(atomic::AtomicU64::new(first_requestid));
        let outgoing = Queue::default().split();
        let publisher = Some(Publisher::new(outgoing.0.clone(), webtransport.clone(), next_requestid.clone()));
        let subscriber = Some(Subscriber::new(outgoing.0, next_requestid));

        let session = Self {
            webtransport,
            sender,
            recver,
            publisher: publisher.clone(),
            subscriber: subscriber.clone(),
            outgoing: outgoing.1,
        };

        (session, publisher, subscriber)
    }

    /// Create an outbound/client QUIC connection, by opening a bi-directional QUIC stream for
    /// MOQT control messaging.  Performs SETUP messaging and version negotiation.
    pub async fn connect(
        mut session: web_transport::Session,
    ) -> Result<(Session, Publisher, Subscriber), SessionError> {
        let control = session.open_bi().await?;
        let mut sender = Writer::new(control.0);
        let mut recver = Reader::new(control.1);

        let versions: setup::Versions = [
            setup::Version::DRAFT_14,
        ].into();

        let client = setup::Client {
            versions: versions.clone(),
            params: Default::default(),
        };

        log::debug!("sending CLIENT_SETUP: {:?}", client);
        sender.encode(&client).await?;

        let server: setup::Server = recver.decode().await?;
        log::debug!("received SERVER_SETUP: {:?}", server);

        // We are the client, so the first request id is 0
        let session = Session::new(session, sender, recver, 0);
        Ok((session.0, session.1.unwrap(), session.2.unwrap()))
    }

    /// Accepts an inbound/server QUIC connection, by accepting a bi-directional QUIC stream for
    /// MOQT control messaging.  Performs SETUP messaging and version negotiation.
    pub async fn accept(
        mut session: web_transport::Session,
    ) -> Result<(Session, Option<Publisher>, Option<Subscriber>), SessionError> {
        let control = session.accept_bi().await?;
        let mut sender = Writer::new(control.0);
        let mut recver = Reader::new(control.1);

        let client: setup::Client = recver.decode().await?;
        log::debug!("received CLIENT_SETUP: {:?}", client);

        let server_versions = setup::Versions(vec![
            setup::Version::DRAFT_14,
        ]);

        if let Some(largest_common_version) = Self::largest_common(&server_versions, &client.versions) {
            let server = setup::Server {
                version: largest_common_version,
                params: Default::default(),
            };

            log::debug!("sending SERVER_SETUP: {:?}", server);
            sender.encode(&server).await?;

            // We are the server, so the first request id is 1
            Ok(Session::new(session, sender, recver, 1))
        } else {
            return Err(SessionError::Version(
                client.versions,
                server_versions,
            ));
        }
    }

    /// Run Tasks for the session, including sending of control messages, receiving and processing
    /// inbound control messages, receiving and processing new inbound uni-directional QUIC streams,
    /// and receiving and processing QUIC datagrams received
    pub async fn run(self) -> Result<(), SessionError> {
        tokio::select! {
            res = Self::run_recv(self.recver, self.publisher, self.subscriber.clone()) => res,
            res = Self::run_send(self.sender, self.outgoing) => res,
            res = Self::run_streams(self.webtransport.clone(), self.subscriber.clone()) => res,
            res = Self::run_datagrams(self.webtransport, self.subscriber) => res,
        }
    }

    /// Processes the outgoing control message queue, and sends queued messages on the control stream sender/writer.
    async fn run_send(
        mut sender: Writer,
        mut outgoing: Queue<message::Message>,
    ) -> Result<(), SessionError> {
        while let Some(msg) = outgoing.pop().await {
            log::debug!("sending message: {:?}", msg);
            sender.encode(&msg).await?;
        }

        Ok(())
    }

    /// Receives inbound messages from the control stream reader/receiver.  Analyzes if the message
    /// is to be handled by Subscriber or Publisher logic and calls recv_message on either the
    /// Publisher or Subscriber.
    /// Note:  Should also be handling messages common to both roles, ie: GOAWAY, MAX_REQUEST_ID and
    ///        REQUESTS_BLOCKED
    async fn run_recv(
        mut recver: Reader,
        mut publisher: Option<Publisher>,
        mut subscriber: Option<Subscriber>,
    ) -> Result<(), SessionError> {
        loop {
            let msg: message::Message = recver.decode().await?;
            log::debug!("received message: {:?}", msg);

            let msg = match TryInto::<message::Publisher>::try_into(msg) {
                Ok(msg) => {
                    subscriber
                        .as_mut()
                        .ok_or(SessionError::RoleViolation)?
                        .recv_message(msg)?;
                    continue;
                }
                Err(msg) => msg,
            };

            let msg = match TryInto::<message::Subscriber>::try_into(msg) {
                Ok(msg) => {
                    publisher
                        .as_mut()
                        .ok_or(SessionError::RoleViolation)?
                        .recv_message(msg)?;
                    continue;
                }
                Err(msg) => msg,
            };

            // TODO GOAWAY, MAX_REQUEST_ID, REQUESTS_BLOCKED
            unimplemented!("unknown message context: {:?}", msg)
        }
    }

    /// Accepts uni-directional quic streams and starts handling for them.
    /// Will read stream header to know what type of stream it is and create
    /// the appropriate stream handlers.
    async fn run_streams(
        mut webtransport: web_transport::Session,
        subscriber: Option<Subscriber>,
    ) -> Result<(), SessionError> {
        let mut tasks = FuturesUnordered::new();

        loop {
            tokio::select! {
                res = webtransport.accept_uni() => {
                    let stream = res?;
                    let subscriber = subscriber.clone().ok_or(SessionError::RoleViolation)?;

                    tasks.push(async move {
                        if let Err(err) = Subscriber::recv_stream(subscriber, stream).await {
                            log::warn!("failed to serve stream: {}", err);
                        };
                    });
                },
                _ = tasks.next(), if !tasks.is_empty() => {},
            };
        }
    }

    /// Receives QUIC datagrams and processes them using the Subscriber logic
    async fn run_datagrams(
        mut webtransport: web_transport::Session,
        mut subscriber: Option<Subscriber>,
    ) -> Result<(), SessionError> {
        loop {
            let datagram = webtransport.recv_datagram().await?;
            subscriber
                .as_mut()
                .ok_or(SessionError::RoleViolation)?
                .recv_datagram(datagram)?;
        }
    }
}
