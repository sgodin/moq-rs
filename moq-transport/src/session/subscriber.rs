use std::{
    collections::{hash_map, HashMap},
    io,
    sync::{atomic, Arc, Mutex},
};

use crate::{
    coding::{Decode, TrackNamespace},
    data,
    message::{self, Message},
    mlog,
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

    /// Optional mlog writer for logging transport events
    mlog: Option<Arc<Mutex<mlog::MlogWriter>>>,
}

impl Subscriber {
    pub(super) fn new(
        outgoing: Queue<Message>,
        next_requestid: Arc<atomic::AtomicU64>,
        mlog: Option<Arc<Mutex<mlog::MlogWriter>>>,
    ) -> Self {
        Self {
            announced: Default::default(),
            announced_queue: Default::default(),
            subscribes: Default::default(),
            outgoing,
            next_requestid,
            mlog,
        }
    }

    pub async fn accept(session: web_transport::Session) -> Result<(Session, Self), SessionError> {
        let (session, _, subscriber) = Session::accept(session, None).await?;
        Ok((session, subscriber.unwrap()))
    }

    pub async fn connect(session: web_transport::Session) -> Result<(Session, Self), SessionError> {
        let (session, _, subscriber) = Session::connect(session, None).await?;
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
        log::trace!("[SUBSCRIBER] recv_stream: new stream received, decoding header");
        let mut reader = Reader::new(stream);

        // Decode the stream header
        let stream_header: data::StreamHeader = reader.decode().await?;
        log::debug!(
            "[SUBSCRIBER] recv_stream: decoded stream header type={:?}",
            stream_header.header_type
        );

        // Log subgroup header parsed/received
        if let Some(ref subgroup_header) = stream_header.subgroup_header {
            if let Some(ref mlog) = self.mlog {
                if let Ok(mut mlog_guard) = mlog.lock() {
                    let time = mlog_guard.elapsed_ms();
                    let stream_id = 0; // TODO: Placeholder, need actual QUIC stream ID
                    let event = mlog::subgroup_header_parsed(time, stream_id, subgroup_header);
                    let _ = mlog_guard.add_event(event);
                }
            }
        }

        // No fetch support yet, so panic if fetch_header for now (via unwrap below)
        // TODO SLG - used to use subscribe_id, using track_alias for now, needs fixing
        let id = stream_header.subgroup_header.as_ref().unwrap().track_alias;
        log::trace!(
            "[SUBSCRIBER] recv_stream: stream for subscription id={}",
            id
        );

        let mlog = self.mlog.clone();
        let res = self.recv_stream_inner(reader, stream_header, mlog).await;
        if let Err(SessionError::Serve(err)) = &res {
            log::warn!(
                "[SUBSCRIBER] recv_stream: stream processing error for id={}: {:?}",
                id,
                err
            );
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
        mlog: Option<Arc<Mutex<mlog::MlogWriter>>>,
    ) -> Result<(), SessionError> {
        // TODO SLG - used to use subscribe_id, using track_alias for now, needs fixing
        let id = stream_header.subgroup_header.as_ref().unwrap().track_alias;
        log::trace!(
            "[SUBSCRIBER] recv_stream_inner: processing stream for id={}",
            id
        );

        // This is super silly, but I couldn't figure out a way to avoid the mutex guard across awaits.
        enum Writer {
            //Fetch(serve::FetchWriter),
            Subgroup(serve::SubgroupWriter),
        }

        let writer = {
            let mut subscribes = self.subscribes.lock().unwrap();
            let subscribe = subscribes.get_mut(&id).ok_or_else(|| {
                log::error!(
                    "[SUBSCRIBER] recv_stream_inner: subscription id={} not found",
                    id
                );
                ServeError::NotFound
            })?;

            if stream_header.header_type.is_subgroup() {
                log::trace!("[SUBSCRIBER] recv_stream_inner: creating subgroup writer");
                Writer::Subgroup(subscribe.subgroup(stream_header.subgroup_header.unwrap())?)
            } else {
                panic!("Fetch not implemented yet!")
            }
        };

        match writer {
            //Writer::Fetch(fetch) => Self::recv_fetch(fetch, reader).await?,
            Writer::Subgroup(subgroup) => {
                log::trace!("[SUBSCRIBER] recv_stream_inner: receiving subgroup data");
                Self::recv_subgroup(stream_header.header_type, subgroup, reader, mlog).await?
            }
        };

        log::debug!(
            "[SUBSCRIBER] recv_stream_inner: completed processing stream for id={}",
            id
        );
        Ok(())
    }

    async fn recv_subgroup(
        stream_header_type: data::StreamHeaderType,
        mut subgroup_writer: serve::SubgroupWriter,
        mut reader: Reader,
        mlog: Option<Arc<Mutex<mlog::MlogWriter>>>,
    ) -> Result<(), SessionError> {
        log::debug!(
            "[SUBSCRIBER] recv_subgroup: starting - group_id={}, subgroup_id={}, priority={}",
            subgroup_writer.info.group_id,
            subgroup_writer.info.subgroup_id,
            subgroup_writer.info.priority
        );

        let mut object_count = 0;
        let mut current_object_id = 0u64;
        while !reader.done().await? {
            log::trace!(
                "[SUBSCRIBER] recv_subgroup: reading object #{} (has_ext_headers={})",
                object_count + 1,
                stream_header_type.has_extension_headers()
            );

            // Need to be able to decode the subgroup object conditionally based on the stream header type
            // read the object payload length into remaining_bytes
            let (mut remaining_bytes, object_id_delta, status, decoded_object) = match stream_header_type
                .has_extension_headers()
            {
                true => {
                    let object = reader.decode::<data::SubgroupObjectExt>().await?;
                    log::debug!(
                        "[SUBSCRIBER] recv_subgroup: object #{} with extension headers - object_id_delta={}, payload_length={}, status={:?}",
                        object_count + 1,
                        object.object_id_delta,
                        object.payload_length,
                        object.status
                    );
                    let obj_copy = object.clone();
                    (object.payload_length, object.object_id_delta, object.status, Some(obj_copy))
                }
                false => {
                    let object = reader.decode::<data::SubgroupObject>().await?;
                    log::debug!(
                        "[SUBSCRIBER] recv_subgroup: object #{} - object_id_delta={}, payload_length={}, status={:?}",
                        object_count + 1,
                        object.object_id_delta,
                        object.payload_length,
                        object.status
                    );
                    (object.payload_length, object.object_id_delta, object.status, None)
                }
            };

            // Calculate absolute object_id from delta
            current_object_id += object_id_delta;

            // Log subgroup object parsed/received
            if let Some(ref mlog) = mlog {
                if let Ok(mut mlog_guard) = mlog.lock() {
                    let time = mlog_guard.elapsed_ms();
                    let stream_id = 0; // TODO: Placeholder, need actual QUIC stream ID
                    let event = if let Some(obj_ext) = decoded_object {
                        mlog::subgroup_object_ext_parsed(
                            time,
                            stream_id,
                            subgroup_writer.info.group_id,
                            subgroup_writer.info.subgroup_id,
                            current_object_id,
                            &obj_ext,
                        )
                    } else {
                        // For non-extension objects, create a temporary SubgroupObject for logging
                        let temp_obj = data::SubgroupObject {
                            object_id_delta,
                            payload_length: remaining_bytes,
                            status,
                        };
                        mlog::subgroup_object_parsed(
                            time,
                            stream_id,
                            subgroup_writer.info.group_id,
                            subgroup_writer.info.subgroup_id,
                            current_object_id,
                            &temp_obj,
                        )
                    };
                    let _ = mlog_guard.add_event(event);
                }
            }

            // TODO SLG - object_id_delta, extension headers and object status are being ignored and not passed on

            let mut object_writer = subgroup_writer.create(remaining_bytes)?;
            log::trace!(
                "[SUBSCRIBER] recv_subgroup: reading payload for object #{} ({} bytes)",
                object_count + 1,
                remaining_bytes
            );

            let mut chunks_read = 0;
            while remaining_bytes > 0 {
                let data = reader
                    .read_chunk(remaining_bytes)
                    .await?
                    .ok_or_else(|| {
                        log::error!(
                            "[SUBSCRIBER] recv_subgroup: ERROR - stream ended with {} bytes remaining for object #{}",
                            remaining_bytes,
                            object_count + 1
                        );
                        SessionError::WrongSize
                    })?;
                log::trace!(
                    "[SUBSCRIBER] recv_subgroup: received payload chunk #{} for object #{} ({} bytes, {} remaining)",
                    chunks_read + 1,
                    object_count + 1,
                    data.len(),
                    remaining_bytes - data.len()
                );
                remaining_bytes -= data.len();
                object_writer.write(data)?;
                chunks_read += 1;
            }

            log::trace!(
                "[SUBSCRIBER] recv_subgroup: completed object #{} ({} chunks)",
                object_count + 1,
                chunks_read
            );
            object_count += 1;
        }

        log::info!(
            "[SUBSCRIBER] recv_subgroup: completed subgroup (group_id={}, subgroup_id={}, {} objects received)",
            subgroup_writer.info.group_id,
            subgroup_writer.info.subgroup_id,
            object_count
        );

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
