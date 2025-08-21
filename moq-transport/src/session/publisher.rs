use std::{
    collections::{hash_map, HashMap},
    sync::{Arc, Mutex},
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

    announces: Arc<Mutex<HashMap<TrackNamespace, AnnounceRecv>>>,
    subscribed: Arc<Mutex<HashMap<u64, SubscribedRecv>>>,
    unknown: Queue<Subscribed>,

    outgoing: Queue<Message>,
}

impl Publisher {
    pub(crate) fn new(outgoing: Queue<Message>, webtransport: web_transport::Session) -> Self {
        Self {
            webtransport,
            announces: Default::default(),
            subscribed: Default::default(),
            unknown: Default::default(),
            outgoing,
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
        let (session, publisher, _) =
            Session::connect(session).await?;
        Ok((session, publisher))
    }

    /// Announce a namespace and serve tracks using the provided [serve::TracksReader].
    /// The caller uses [serve::TracksWriter] for static tracks and [serve::TracksRequest] for dynamic tracks.
    pub async fn announce(&mut self, tracks: TracksReader) -> Result<(), SessionError> {
        let announce = match self
            .announces
            .lock()
            .unwrap()
            .entry(tracks.namespace.clone())
        {
            hash_map::Entry::Occupied(_) => return Err(ServeError::Duplicate.into()),
            hash_map::Entry::Vacant(entry) => {
                let (send, recv) = Announce::new(self.clone(), tracks.namespace.clone());
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
                expires: 3600, // TODO SLG
                group_order: GroupOrder::Ascending, // TODO SLG
                content_exists: true,
                largest_location: Some(latest),
                params: Default::default(),
            };
        } else {
            response = message::TrackStatusOk {
                id: track_status_request.request_msg.id,
                track_alias: 0, // TODO SLG - wire up track alias logic
                expires: 3600, // TODO SLG
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
            message::Subscriber::AnnounceOk(msg) => self.recv_announce_ok(msg),
            message::Subscriber::AnnounceError(msg) => self.recv_announce_error(msg),
            message::Subscriber::AnnounceCancel(msg) => self.recv_announce_cancel(msg),
            message::Subscriber::Subscribe(msg) => self.recv_subscribe(msg),
            message::Subscriber::Unsubscribe(msg) => self.recv_unsubscribe(msg),
            message::Subscriber::SubscribeUpdate(msg) => self.recv_subscribe_update(msg),
            message::Subscriber::TrackStatus(msg) => self.recv_track_status(msg),
            // TODO: Implement namespace messages.
            message::Subscriber::SubscribeNamespace(_msg) => unimplemented!(),
            message::Subscriber::SubscribeNamespaceOk(_msg) => unimplemented!(),
            message::Subscriber::SubscribeNamespaceError(_msg) => unimplemented!(),
            message::Subscriber::UnsubscribeNamespace(_msg) => unimplemented!(),
            // TODO: Implement fetch messages
            message::Subscriber::Fetch(_msg) => todo!(),
            message::Subscriber::FetchCancel(_msg) => todo!(),
            // TODO: Implement publish messages
            message::Subscriber::PublishOk(_msg) => todo!(),
            message::Subscriber::PublishError(_msg) => todo!(),
        };

        if let Err(err) = res {
            log::warn!("failed to process message: {}", err);
        }

        Ok(())
    }

    fn recv_announce_ok(&mut self, _msg: message::AnnounceOk) -> Result<(), SessionError> {
        // TODO SLG - need to map msg.id to announces which are indexed by namespace
        //if let Some(announce) = self.announces.lock().unwrap().get_mut(&msg.namespace) {
        //    announce.recv_ok()?;
        //}

        Ok(())
    }

    fn recv_announce_error(&mut self, _msg: message::AnnounceError) -> Result<(), SessionError> {
        // TODO SLG - need to map msg.id to announces which are indexed by namespace
        //if let Some(announce) = self.announces.lock().unwrap().remove(&msg.namespace) {
        //    announce.recv_error(ServeError::Closed(msg.error_code))?;
        //}

        Ok(())
    }

    fn recv_announce_cancel(&mut self, msg: message::AnnounceCancel) -> Result<(), SessionError> {
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

    fn recv_track_status(
        &mut self,
        msg: message::TrackStatus,
    ) -> Result<(), SessionError> {
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
            message::Publisher::SubscribeDone(msg) => self.drop_subscribe(msg.id),
            message::Publisher::SubscribeError(msg) => self.drop_subscribe(msg.id),
            message::Publisher::Unannounce(msg) => self.drop_announce(&msg.track_namespace),
            _ => (),
        };

        self.outgoing.push(msg.into()).ok();
    }

    fn drop_subscribe(&mut self, id: u64) {
        self.subscribed.lock().unwrap().remove(&id);
    }

    fn drop_announce(&mut self, namespace: &TrackNamespace) {
        self.announces.lock().unwrap().remove(namespace);
    }

    pub(super) async fn open_uni(&mut self) -> Result<web_transport::SendStream, SessionError> {
        Ok(self.webtransport.open_uni().await?)
    }

    pub(super) async fn send_datagram(&mut self, data: bytes::Bytes) -> Result<(), SessionError> {
        Ok(self.webtransport.send_datagram(data).await?)
    }
}
