use std::{
    collections::{hash_map, HashMap},
    sync::{atomic, Arc, Mutex},
};

use futures::{stream::FuturesUnordered, StreamExt};

use crate::{
    coding::TrackNamespace,
    message::{self, GroupOrder, Message},
    serve::{ServeError, TracksReader},
};

use crate::watch::Queue;

use super::{
    Announce, AnnounceRecv, Session, SessionError, Subscribed, SubscribedRecv, TrackStatusRequested,
};

// TODO remove Clone.
#[derive(Clone)]
pub struct Publisher {
    webtransport: web_transport::Session,

    /// When the announce method is used, a new entry is added to this HashMap to track outbound announcement
    announces: Arc<Mutex<HashMap<TrackNamespace, AnnounceRecv>>>,

    /// When a Subscribe is received and we have a previous annouce for the namespace, then a new entry is
    /// added to this HashMap to track the inbound subscription
    subscribed: Arc<Mutex<HashMap<u64, SubscribedRecv>>>,

    /// When a Subscribe is received and we DO NOT have a previous annouce for the namespace, then a new entry is
    /// added to this Queue to track the inbound subscription
    unknown: Queue<Subscribed>,

    /// The queue we will write any outbound control messages we want to sent, the session run_send task
    /// will process the queue and send the message on the control stream.
    outgoing: Queue<Message>,

    /// When we need a new Request Id for sending a request, we can get it from here.  Note:  The instance
    /// of AtomicU64 is shared with the Subscriber, so the session uses unique request ids for all requests
    /// generated.  Note:  If we initiated the QUIC connection then request id's start at 0 and increment by 2
    /// for each request (even numbers).  If we accepted an inbound QUIC connection then request id's start at 1 and
    /// increment by 2 for each request (odd numbers).
    next_requestid: Arc<atomic::AtomicU64>,
}

impl Publisher {
    pub(crate) fn new(
        outgoing: Queue<Message>,
        webtransport: web_transport::Session,
        next_requestid: Arc<atomic::AtomicU64>,
    ) -> Self {
        Self {
            webtransport,
            announces: Default::default(),
            subscribed: Default::default(),
            unknown: Default::default(),
            outgoing,
            next_requestid,
        }
    }

    pub async fn accept(
        session: web_transport::Session,
    ) -> Result<(Session, Publisher), SessionError> {
        let (session, publisher, _) = Session::accept(session).await?;
        Ok((session, publisher.unwrap()))
    }

    pub async fn connect(
        session: web_transport::Session,
    ) -> Result<(Session, Publisher), SessionError> {
        let (session, publisher, _) = Session::connect(session).await?;
        Ok((session, publisher))
    }

    /// Announce a namespace and serve tracks using the provided [serve::TracksReader].
    /// The caller uses [serve::TracksWriter] for static tracks and [serve::TracksRequest] for dynamic tracks.
    pub async fn announce(&mut self, tracks: TracksReader) -> Result<(), SessionError> {
        // Check if annouce for this namespace already exists or not, and if not, then create a new Announce
        let announce = match self
            .announces
            .lock()
            .unwrap()
            .entry(tracks.namespace.clone())
        {
            // Namespace already exists in HashMap (has already been announced) - return Duplicate error
            hash_map::Entry::Occupied(_) => return Err(ServeError::Duplicate.into()),

            // This is a new announce, send announce message to peer.
            hash_map::Entry::Vacant(entry) => {
                // Get the current next request id to use and increment the value for by 2 for the next request
                let request_id = self.next_requestid.fetch_add(2, atomic::Ordering::Relaxed);

                let (send, recv) =
                    Announce::new(self.clone(), request_id, tracks.namespace.clone());
                entry.insert(recv);
                send
            }
        };

        let mut subscribe_tasks = FuturesUnordered::new();
        let mut status_tasks = FuturesUnordered::new();
        let mut subscribe_done = false;
        let mut status_done = false;

        loop {
            tokio::select! {
                res = announce.subscribed(), if !subscribe_done => {
                    match res? {
                        Some(subscribed) => {
                            let tracks = tracks.clone();

                            subscribe_tasks.push(async move {
                                let info = subscribed.info.clone();
                                if let Err(err) = Self::serve_subscribe(subscribed, tracks).await {
                                    log::warn!("failed serving subscribe: {:?}, error: {}", info, err)
                                }
                            });
                        },
                        None => subscribe_done = true,
                    }

                },
                res = announce.track_status_requested(), if !status_done => {
                    match res? {
                        Some(status) => {
                            let tracks = tracks.clone();

                            status_tasks.push(async move {
                                let request_msg = status.request_msg.clone();
                                if let Err(err) = Self::serve_track_status(status, tracks).await {
                                    log::warn!("failed serving track status request: {:?}, error: {}", request_msg, err)
                                }
                            });
                        },
                        None => status_done = true,
                    }
                },
                Some(res) = subscribe_tasks.next() => res,
                Some(res) = status_tasks.next() => res,
                else => return Ok(())
            }
        }
    }

    pub async fn serve_subscribe(
        subscribe: Subscribed,
        mut tracks: TracksReader,
    ) -> Result<(), SessionError> {
        if let Some(track) = tracks.subscribe(&subscribe.name) {
            subscribe.serve(track).await?;
        } else {
            subscribe.close(ServeError::NotFound)?;
        }

        Ok(())
    }

    pub async fn serve_track_status(
        mut track_status_request: TrackStatusRequested,
        mut tracks: TracksReader,
    ) -> Result<(), SessionError> {
        let track = tracks
            .subscribe(&track_status_request.request_msg.track_name.clone())
            .ok_or(ServeError::NotFound)?;
        let response;

        if let Some(latest) = track.latest() {
            response = message::TrackStatusOk {
                id: track_status_request.request_msg.id,
                track_alias: 0, // TODO SLG - wire up track alias logic
                expires: 3600,  // TODO SLG
                group_order: GroupOrder::Ascending, // TODO SLG
                content_exists: true,
                largest_location: Some(latest),
                params: Default::default(),
            };
        } else {
            response = message::TrackStatusOk {
                id: track_status_request.request_msg.id,
                track_alias: 0, // TODO SLG - wire up track alias logic
                expires: 3600,  // TODO SLG
                group_order: GroupOrder::Ascending, // TODO SLG
                content_exists: false,
                largest_location: None,
                params: Default::default(),
            };
        }

        // TODO: can we know of any other statuses in this context?

        track_status_request.respond(response).await?;

        Ok(())
    }

    // Returns subscriptions that do not map to an active announce.
    pub async fn subscribed(&mut self) -> Option<Subscribed> {
        self.unknown.pop().await
    }

    pub(crate) fn recv_message(&mut self, msg: message::Subscriber) -> Result<(), SessionError> {
        let res = match msg {
            message::Subscriber::Subscribe(msg) => self.recv_subscribe(msg),
            message::Subscriber::SubscribeUpdate(msg) => self.recv_subscribe_update(msg),
            message::Subscriber::Unsubscribe(msg) => self.recv_unsubscribe(msg),
            message::Subscriber::Fetch(_msg) => todo!(), // TODO
            message::Subscriber::FetchCancel(_msg) => todo!(), // TODO
            message::Subscriber::TrackStatus(msg) => self.recv_track_status(msg),
            message::Subscriber::SubscribeNamespace(_msg) => todo!(), // TODO
            message::Subscriber::UnsubscribeNamespace(_msg) => todo!(), // TODO
            message::Subscriber::PublishNamespaceCancel(msg) => {
                self.recv_publish_namespace_cancel(msg)
            }
            message::Subscriber::PublishNamespaceOk(msg) => self.recv_publish_namespace_ok(msg),
            message::Subscriber::PublishNamespaceError(msg) => {
                self.recv_publish_namespace_error(msg)
            }
            message::Subscriber::PublishOk(_msg) => todo!(), // TODO
            message::Subscriber::PublishError(_msg) => todo!(), // TODO
        };

        if let Err(err) = res {
            log::warn!("failed to process message: {}", err);
        }

        Ok(())
    }

    fn recv_publish_namespace_ok(
        &mut self,
        msg: message::PublishNamespaceOk,
    ) -> Result<(), SessionError> {
        // We need to find the announce request using the request id, however the self.announces data structure
        // is a HashMap indexed by Namespace (which is needed for handling PUBLISH_NAMESPACE_CANCEL).  TODO - make more efficient.
        // For now iterate through all self.annouces until we find the matching id.
        let mut announces = self.announces.lock().unwrap();
        let announce = announces.iter_mut().find(|(_k, v)| v.request_id == msg.id);

        if let Some(announce) = announce {
            announce.1.recv_ok()?;
        }

        Ok(())
    }

    fn recv_publish_namespace_error(
        &mut self,
        msg: message::PublishNamespaceError,
    ) -> Result<(), SessionError> {
        // We need to find the announce request using the request id, however the self.announces data structure
        // is a HashMap indexed by Namespace (which is needed for handling PUBLISH_NAMESPACE_CANCEL).  TODO - make more efficient.
        // For now iterate through all self.annouces until we find the matching id.
        let mut announces = self.announces.lock().unwrap();

        // Find the key first (immutable borrow only)
        let key_opt = announces
            .iter()
            .find(|(_k, v)| v.request_id == msg.id)
            .map(|(k, _)| k.clone());

        // Remove from HashMap and take ownership
        if let Some(key) = key_opt {
            if let Some((_ns, v)) = announces.remove_entry(&key) {
                // Step 3: call recv_error, consuming v
                v.recv_error(ServeError::Closed(msg.error_code))?;
            }
        }

        Ok(())
    }

    fn recv_publish_namespace_cancel(
        &mut self,
        msg: message::PublishNamespaceCancel,
    ) -> Result<(), SessionError> {
        // TODO: If a publisher receives new subscriptions for that namespace after receiving an ANNOUNCE_CANCEL,
        // it SHOULD close the session as a 'Protocol Violation'.
        if let Some(announce) = self.announces.lock().unwrap().remove(&msg.track_namespace) {
            announce.recv_error(ServeError::Cancel)?;
        }

        Ok(())
    }

    fn recv_subscribe(&mut self, msg: message::Subscribe) -> Result<(), SessionError> {
        let namespace = msg.track_namespace.clone();

        let subscribe = {
            let mut subscribes = self.subscribed.lock().unwrap();

            // Insert the abort handle into the lookup table.
            let entry = match subscribes.entry(msg.id) {
                hash_map::Entry::Occupied(_) => return Err(SessionError::Duplicate),
                hash_map::Entry::Vacant(entry) => entry,
            };

            let (send, recv) = Subscribed::new(self.clone(), msg);
            entry.insert(recv);

            send
        };

        // If we have an announce, route the subscribe to it.
        if let Some(announce) = self.announces.lock().unwrap().get_mut(&namespace) {
            return announce.recv_subscribe(subscribe).map_err(Into::into);
        }

        // Otherwise, put it in the unknown queue.
        // TODO Have some way to detect if the application is not reading from the unknown queue.
        if let Err(err) = self.unknown.push(subscribe) {
            // Default to closing with a not found error I guess.
            err.close(ServeError::NotFound)?;
        }

        Ok(())
    }

    fn recv_subscribe_update(
        &mut self,
        _msg: message::SubscribeUpdate,
    ) -> Result<(), SessionError> {
        // TODO: Implement updating subscriptions.
        Err(SessionError::Internal)
    }

    fn recv_track_status(&mut self, msg: message::TrackStatus) -> Result<(), SessionError> {
        let namespace = msg.track_namespace.clone();

        let mut announces = self.announces.lock().unwrap();
        let announce = announces
            .get_mut(&namespace)
            .ok_or(SessionError::Internal)?;

        let track_status_requested = TrackStatusRequested::new(self.clone(), msg);

        announce
            .recv_track_status_requested(track_status_requested)
            .map_err(Into::into)
    }

    fn recv_unsubscribe(&mut self, msg: message::Unsubscribe) -> Result<(), SessionError> {
        if let Some(subscribed) = self.subscribed.lock().unwrap().get_mut(&msg.id) {
            subscribed.recv_unsubscribe()?;
        }

        Ok(())
    }

    pub(super) fn send_message<T: Into<message::Publisher> + Into<Message>>(&mut self, msg: T) {
        let msg = msg.into();
        match &msg {
            message::Publisher::PublishDone(msg) => self.drop_subscribe(msg.id),
            message::Publisher::SubscribeError(msg) => self.drop_subscribe(msg.id),
            message::Publisher::PublishNamespaceDone(msg) => {
                self.drop_publish_namespace(&msg.track_namespace)
            }
            _ => (),
        };

        self.outgoing.push(msg.into()).ok();
    }

    fn drop_subscribe(&mut self, id: u64) {
        self.subscribed.lock().unwrap().remove(&id);
    }

    fn drop_publish_namespace(&mut self, namespace: &TrackNamespace) {
        self.announces.lock().unwrap().remove(namespace);
    }

    pub(super) async fn open_uni(&mut self) -> Result<web_transport::SendStream, SessionError> {
        Ok(self.webtransport.open_uni().await?)
    }

    pub(super) async fn send_datagram(&mut self, data: bytes::Bytes) -> Result<(), SessionError> {
        Ok(self.webtransport.send_datagram(data).await?)
    }
}
