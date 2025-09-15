use std::{
    collections::{hash_map, HashMap},
    io,
    sync::{atomic, Arc, Mutex},
};

use crate::{
    coding::{Decode, TrackNamespace},
    data,
    message::{self, Message},
    serve::{self, ServeError},
};

use crate::watch::Queue;

use super::{Announced, AnnouncedRecv, Reader, Session, SessionError, Subscribe, SubscribeRecv};

// TODO remove Clone.
#[derive(Clone)]
pub struct Subscriber {
    announced: Arc<Mutex<HashMap<TrackNamespace, AnnouncedRecv>>>,
    announced_queue: Queue<Announced>,

    subscribes: Arc<Mutex<HashMap<u64, SubscribeRecv>>>,

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

impl Subscriber {
    pub(super) fn new(outgoing: Queue<Message>, next_requestid: Arc<atomic::AtomicU64>) -> Self {
        Self {
            announced: Default::default(),
            announced_queue: Default::default(),
            subscribes: Default::default(),
            outgoing,
            next_requestid,
        }
    }

    pub async fn accept(session: web_transport::Session) -> Result<(Session, Self), SessionError> {
        let (session, _, subscriber) = Session::accept(session).await?;
        Ok((session, subscriber.unwrap()))
    }

    pub async fn connect(session: web_transport::Session) -> Result<(Session, Self), SessionError> {
        let (session, _, subscriber) = Session::connect(session).await?;
        Ok((session, subscriber))
    }

    pub async fn announced(&mut self) -> Option<Announced> {
        self.announced_queue.pop().await
    }

    pub async fn subscribe(&mut self, track: serve::TrackWriter) -> Result<(), ServeError> {
        // Get the current next request id to use and increment the value for by 2 for the next request
        let request_id = self.next_requestid.fetch_add(2, atomic::Ordering::Relaxed);

        let (send, recv) = Subscribe::new(self.clone(), request_id, track);
        self.subscribes.lock().unwrap().insert(request_id, recv);

        send.closed().await
    }

    pub(super) fn send_message<M: Into<message::Subscriber>>(&mut self, msg: M) {
        let msg = msg.into();

        // Remove our entry on terminal state.
        match &msg {
            message::Subscriber::PublishNamespaceCancel(msg) => {
                self.drop_publish_namespace(&msg.track_namespace)
            }
            // TODO SLG - there is no longer a namespace in the error, need to map via request id
            message::Subscriber::PublishNamespaceError(_msg) => todo!(), //self.drop_announce(&msg.track_namespace),
            _ => {}
        }

        // TODO report dropped messages?
        let _ = self.outgoing.push(msg.into());
    }

    pub(super) fn recv_message(&mut self, msg: message::Publisher) -> Result<(), SessionError> {
        let res = match &msg {
            message::Publisher::PublishNamespace(msg) => self.recv_publish_namespace(msg),
            message::Publisher::PublishNamespaceDone(msg) => self.recv_publish_namespace_done(msg),
            message::Publisher::Publish(_msg) => todo!(), // TODO
            message::Publisher::PublishDone(msg) => self.recv_publish_done(msg),
            message::Publisher::SubscribeOk(msg) => self.recv_subscribe_ok(msg),
            message::Publisher::SubscribeError(msg) => self.recv_subscribe_error(msg),
            message::Publisher::TrackStatusOk(msg) => self.recv_track_status_ok(msg),
            message::Publisher::TrackStatusError(_msg) => todo!(), // TODO
            message::Publisher::FetchOk(_msg) => todo!(),          // TODO
            message::Publisher::FetchError(_msg) => todo!(),       // TODO
            message::Publisher::SubscribeNamespaceOk(_msg) => todo!(),
            message::Publisher::SubscribeNamespaceError(_msg) => todo!(),
        };

        if let Err(SessionError::Serve(err)) = res {
            log::debug!("failed to process message: {:?} {}", msg, err);
            return Ok(());
        }

        res
    }

    fn recv_publish_namespace(
        &mut self,
        msg: &message::PublishNamespace,
    ) -> Result<(), SessionError> {
        let mut announces = self.announced.lock().unwrap();

        let entry = match announces.entry(msg.track_namespace.clone()) {
            hash_map::Entry::Occupied(_) => return Err(SessionError::Duplicate),
            hash_map::Entry::Vacant(entry) => entry,
        };

        let (announced, recv) = Announced::new(self.clone(), msg.id, msg.track_namespace.clone());
        if let Err(announced) = self.announced_queue.push(announced) {
            announced.close(ServeError::Cancel)?;
            return Ok(());
        }

        entry.insert(recv);

        Ok(())
    }

    fn recv_publish_namespace_done(
        &mut self,
        msg: &message::PublishNamespaceDone,
    ) -> Result<(), SessionError> {
        if let Some(announce) = self.announced.lock().unwrap().remove(&msg.track_namespace) {
            announce.recv_unannounce()?;
        }

        Ok(())
    }

    fn recv_subscribe_ok(&mut self, msg: &message::SubscribeOk) -> Result<(), SessionError> {
        if let Some(subscribe) = self.subscribes.lock().unwrap().get_mut(&msg.id) {
            subscribe.ok()?;
        }

        Ok(())
    }

    fn recv_subscribe_error(&mut self, msg: &message::SubscribeError) -> Result<(), SessionError> {
        if let Some(subscribe) = self.subscribes.lock().unwrap().remove(&msg.id) {
            subscribe.error(ServeError::Closed(msg.error_code))?;
        }

        Ok(())
    }

    fn recv_publish_done(&mut self, msg: &message::PublishDone) -> Result<(), SessionError> {
        if let Some(subscribe) = self.subscribes.lock().unwrap().remove(&msg.id) {
            subscribe.error(ServeError::Closed(msg.status_code))?;
        }

        Ok(())
    }

    fn recv_track_status_ok(&mut self, _msg: &message::TrackStatusOk) -> Result<(), SessionError> {
        // TODO: Expose this somehow?
        // TODO: Also add a way to send a Track Status Request in the first place

        Ok(())
    }

    fn drop_publish_namespace(&mut self, namespace: &TrackNamespace) {
        self.announced.lock().unwrap().remove(namespace);
    }

    pub(super) async fn recv_stream(
        mut self,
        stream: web_transport::RecvStream,
    ) -> Result<(), SessionError> {
        let mut reader = Reader::new(stream);

        // Decode the stream header
        let stream_header: data::StreamHeader = reader.decode().await?;

        // No fetch support yet, so panic if fetch_header for now (via unwrap below)
        // TODO SLG - used to use subscribe_id, using track_alias for now, needs fixing
        let id = stream_header.subgroup_header.as_ref().unwrap().track_alias;

        let res = self.recv_stream_inner(reader, stream_header).await;
        if let Err(SessionError::Serve(err)) = &res {
            // The writer is closed, so we should teriminate.
            // TODO it would be nice to do this immediately when the Writer is closed.
            if let Some(subscribe) = self.subscribes.lock().unwrap().remove(&id) {
                subscribe.error(err.clone())?;
            }
        }

        res
    }

    async fn recv_stream_inner(
        &mut self,
        reader: Reader,
        stream_header: data::StreamHeader,
    ) -> Result<(), SessionError> {
        // No fetch support yet, so panic if fetch_header for now (via unwrap below)
        // TODO SLG - used to use subscribe_id, using track_alias for now, needs fixing
        let id = stream_header.subgroup_header.as_ref().unwrap().track_alias;

        // This is super silly, but I couldn't figure out a way to avoid the mutex guard across awaits.
        enum Writer {
            //Fetch(serve::FetchWriter),
            Subgroup(serve::SubgroupWriter),
        }

        let writer = {
            let mut subscribes = self.subscribes.lock().unwrap();
            let subscribe = subscribes.get_mut(&id).ok_or(ServeError::NotFound)?;

            if stream_header.header_type.is_subgroup() {
                Writer::Subgroup(subscribe.subgroup(stream_header.subgroup_header.unwrap())?)
            } else {
                panic!("Fetch not implemented yet!")
            }
        };

        match writer {
            //Writer::Fetch(fetch) => Self::recv_fetch(fetch, reader).await?,
            Writer::Subgroup(subgroup) => {
                Self::recv_subgroup(stream_header.header_type, subgroup, reader).await?
            }
        };

        Ok(())
    }

    async fn recv_subgroup(
        stream_header_type: data::StreamHeaderType,
        mut subgroup_writer: serve::SubgroupWriter,
        mut reader: Reader,
    ) -> Result<(), SessionError> {
        log::trace!("received subgroup: {:?}", subgroup_writer.info);

        while !reader.done().await? {
            // Need to be able to decode the subgroup object conditionally based on the stream header type
            // read the object payload length into remaining_bytes
            let mut remaining_bytes = match stream_header_type.has_extension_headers() {
                true => {
                    let object = reader.decode::<data::SubgroupObjectExt>().await?;
                    log::trace!(
                        "received subgroup object with extension headers: {:?}",
                        object
                    );
                    object.payload_length
                }
                false => {
                    let object = reader.decode::<data::SubgroupObject>().await?;
                    log::trace!("received subgroup object: {:?}", object);
                    object.payload_length
                }
            };

            // TODO SLG - object_id_delta, extension headers and object status are being ignored and not passed on

            let mut object_writer = subgroup_writer.create(remaining_bytes)?;

            while remaining_bytes > 0 {
                let data = reader
                    .read_chunk(remaining_bytes)
                    .await?
                    .ok_or(SessionError::WrongSize)?;
                //log::trace!("received subgroup payload: {:?}", data.len());
                remaining_bytes -= data.len();
                object_writer.write(data)?;
            }
        }

        Ok(())
    }

    pub fn recv_datagram(&mut self, datagram: bytes::Bytes) -> Result<(), SessionError> {
        let mut cursor = io::Cursor::new(datagram);
        let datagram = data::Datagram::decode(&mut cursor)?;

        if let Some(subscribe) = self
            .subscribes
            .lock()
            .unwrap()
            .get_mut(&datagram.track_alias)
        // TODO SLG - look up subscription with track_alias, not subscription id - fix me!
        {
            subscribe.datagram(datagram)?;
        }

        Ok(())
    }
}
