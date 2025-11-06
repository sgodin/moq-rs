use std::{
    collections::{hash_map, HashMap},
    io,
    sync::{atomic, Arc, Mutex},
};

use crate::{
    coding::{Decode, TrackNamespace},
    data,
    message::{self, FilterType, GroupOrder, Message},
    mlog,
    serve::{self, ServeError},
};

use crate::watch::Queue;

use super::{Announced, AnnouncedRecv, Reader, Session, SessionError, Subscribe, SubscribeRecv};

// TODO remove Clone.
#[derive(Clone)]
pub struct Subscriber {
    /// The currently active inbound announces, keyed by namespace.
    announced: Arc<Mutex<HashMap<TrackNamespace, AnnouncedRecv>>>,

    /// Queue of announced namespaces we have received from the Publisher, waiting to be processed.
    announced_queue: Queue<Announced>,

    /// The currently active outbound subscribes, keyed by request id.
    subscribes: Arc<Mutex<HashMap<u64, SubscribeRecv>>>,

    /// Map of track alias to subscription id for quick lookup when receiving streams/datagrams.
    subscribe_alias_map: Arc<Mutex<HashMap<u64, u64>>>,

    /// The queue we will write any outbound control messages we want to send, the session run_send task
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
            subscribe_alias_map: Default::default(),
            outgoing,
            next_requestid,
            mlog,
        }
    }

    /// Create an inbound/server QUIC connection, by accepting a bi-directional QUIC stream for control messages.
    pub async fn accept(session: web_transport::Session) -> Result<(Session, Self), SessionError> {
        let (session, _, subscriber) = Session::accept(session, None).await?;
        Ok((session, subscriber.unwrap()))
    }

    /// Create an outbound/client QUIC connection, by opening a bi-directional QUIC stream for control messages.
    pub async fn connect(session: web_transport::Session) -> Result<(Session, Self), SessionError> {
        let (session, _, subscriber) = Session::connect(session, None).await?;
        Ok((session, subscriber))
    }

    /// Wait for the next announced namespace from the publisher, if any.
    pub async fn announced(&mut self) -> Option<Announced> {
        self.announced_queue.pop().await
    }

    /// Get the current next request id to use and increment the value for by 2 for the next request
    fn get_next_request_id(&self) -> u64 {
        self.next_requestid.fetch_add(2, atomic::Ordering::Relaxed)
    }

    pub fn track_status(&mut self, track_namespace: &TrackNamespace, track_name: &str) {
        self.send_message(message::TrackStatus {
            id: self.get_next_request_id(),
            track_namespace: track_namespace.clone(),
            track_name: track_name.to_string(),
            subscriber_priority: 127, // default to mid value, see: https://github.com/moq-wg/moq-transport/issues/504
            group_order: GroupOrder::Publisher, // defer to publisher send order
            forward: true,            // default to forwarding objects
            filter_type: FilterType::LargestObject,
            start_location: None,
            end_group_id: None,
            params: Default::default(),
        });
        // TODO make async and wait for response?
    }

    /// Subscribe to a track by creating a new subscribe request to the publisher.  Block until subscription is closed.
    pub async fn subscribe(&mut self, track: serve::TrackWriter) -> Result<(), ServeError> {
        let request_id = self.get_next_request_id();
        let (send, recv) = Subscribe::new(self.clone(), request_id, track);
        self.subscribes.lock().unwrap().insert(request_id, recv);

        send.closed().await
    }

    /// Send a message to the publisher via the control stream.
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

    fn not_implemented_yet(&self) -> Result<(), SessionError> {
        Err(SessionError::Serve(ServeError::Internal(
            "Not implemented yet".to_string(),
        )))
    }

    /// Receive a message from the publisher via the control stream.
    pub(super) fn recv_message(&mut self, msg: message::Publisher) -> Result<(), SessionError> {
        let res = match &msg {
            message::Publisher::PublishNamespace(msg) => self.recv_publish_namespace(msg),
            message::Publisher::PublishNamespaceDone(msg) => self.recv_publish_namespace_done(msg),
            message::Publisher::Publish(_msg) => self.not_implemented_yet(), // TODO
            message::Publisher::PublishDone(msg) => self.recv_publish_done(msg),
            message::Publisher::SubscribeOk(msg) => self.recv_subscribe_ok(msg),
            message::Publisher::SubscribeError(msg) => self.recv_subscribe_error(msg),
            message::Publisher::TrackStatusOk(msg) => self.recv_track_status_ok(msg),
            message::Publisher::TrackStatusError(_msg) => self.not_implemented_yet(), // TODO
            message::Publisher::FetchOk(_msg) => self.not_implemented_yet(),          // TODO
            message::Publisher::FetchError(_msg) => self.not_implemented_yet(),       // TODO
            message::Publisher::SubscribeNamespaceOk(_msg) => self.not_implemented_yet(), // TODO
            message::Publisher::SubscribeNamespaceError(_msg) => self.not_implemented_yet(), // TODO
        };

        if let Err(SessionError::Serve(err)) = res {
            log::debug!("failed to process message: {:?} {}", msg, err);
            return Ok(());
        }

        res
    }

    /// Handle the reception of a PublishNamespace message from the publisher.
    fn recv_publish_namespace(
        &mut self,
        msg: &message::PublishNamespace,
    ) -> Result<(), SessionError> {
        let mut announces = self.announced.lock().unwrap();

        // Check for duplicate namespace announcement
        let entry = match announces.entry(msg.track_namespace.clone()) {
            hash_map::Entry::Occupied(_) => return Err(SessionError::Duplicate),
            hash_map::Entry::Vacant(entry) => entry,
        };

        // Create the announced namespace and insert it into our map of active announces, and the announced queue.
        let (announced, recv) = Announced::new(self.clone(), msg.id, msg.track_namespace.clone());
        if let Err(announced) = self.announced_queue.push(announced) {
            announced.close(ServeError::Cancel)?;
            return Ok(());
        }
        entry.insert(recv);

        Ok(())
    }

    /// Handle the reception of a PublishNamespaceDone message from the publisher.
    fn recv_publish_namespace_done(
        &mut self,
        msg: &message::PublishNamespaceDone,
    ) -> Result<(), SessionError> {
        if let Some(announce) = self.announced.lock().unwrap().remove(&msg.track_namespace) {
            announce.recv_unannounce()?;
        }

        Ok(())
    }

    /// Handle the reception of a SubscribeOk message from the publisher.
    fn recv_subscribe_ok(&mut self, msg: &message::SubscribeOk) -> Result<(), SessionError> {
        if let Some(subscribe) = self.subscribes.lock().unwrap().get_mut(&msg.id) {
            // Map track alias to subscription id for quick lookup when receiving streams/datagrams
            self.subscribe_alias_map
                .lock()
                .unwrap()
                .insert(msg.track_alias, msg.id);

            // Notify the subscribe of the successful subscription
            subscribe.ok(msg.track_alias)?;
        }

        Ok(())
    }

    /// Remove a subscribe from our map of active subscribes, and the alias map if present.
    fn remove_subscribe(&mut self, id: u64) -> Option<SubscribeRecv> {
        if let Some(subscribe) = self.subscribes.lock().unwrap().remove(&id) {
            // Remove from alias map if present
            if let Some(track_alias) = subscribe.track_alias() {
                self.subscribe_alias_map
                    .lock()
                    .unwrap()
                    .remove(&track_alias);
            };
            Some(subscribe)
        } else {
            None
        }
    }

    /// Handle the reception of a SubscribeError message from the publisher.
    fn recv_subscribe_error(&mut self, msg: &message::SubscribeError) -> Result<(), SessionError> {
        if let Some(subscribe) = self.remove_subscribe(msg.id) {
            subscribe.error(ServeError::Closed(msg.error_code))?;
        }

        Ok(())
    }

    /// Handle the reception of a PublishDone message from the publisher.
    fn recv_publish_done(&mut self, msg: &message::PublishDone) -> Result<(), SessionError> {
        if let Some(subscribe) = self.remove_subscribe(msg.id) {
            subscribe.error(ServeError::Closed(msg.status_code))?;
        }

        Ok(())
    }

    /// Handle the reception of a TrackStatusOk message from the publisher.
    fn recv_track_status_ok(&mut self, _msg: &message::TrackStatusOk) -> Result<(), SessionError> {
        // TODO: Expose this somehow?
        // TODO: Also add a way to send a Track Status Request in the first place

        Ok(())
    }

    /// Remove an announced namespace from our map of active announces.
    fn drop_publish_namespace(&mut self, namespace: &TrackNamespace) {
        self.announced.lock().unwrap().remove(namespace);
    }

    /// Get a subscribe id by track alias.
    fn get_subscribe_id_by_alias(&mut self, track_alias: u64) -> Option<u64> {
        self.subscribe_alias_map
            .lock()
            .unwrap()
            .get(&track_alias)
            .cloned()
    }

    /// Handle reception of a new stream from the QUIC session.
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
        let track_alias = stream_header.subgroup_header.as_ref().unwrap().track_alias;
        log::trace!(
            "[SUBSCRIBER] recv_stream: stream for subscription track_alias={}",
            track_alias
        );

        let mlog = self.mlog.clone();
        let res = self.recv_stream_inner(reader, stream_header, mlog).await;
        if let Err(SessionError::Serve(err)) = &res {
            log::warn!(
                "[SUBSCRIBER] recv_stream: stream processing error for track_alias={}: {:?}",
                track_alias,
                err
            );
            // The writer is closed, so we should teriminate.
            // TODO it would be nice to do this immediately when the Writer is closed.
            if let Some(subscribe_id) = self.get_subscribe_id_by_alias(track_alias) {
                if let Some(subscribe) = self.remove_subscribe(subscribe_id) {
                    subscribe.error(err.clone())?;
                }
            }
        }

        res
    }

    /// Continue handling the reception of a new stream from the QUIC session.
    async fn recv_stream_inner(
        &mut self,
        reader: Reader,
        stream_header: data::StreamHeader,
        mlog: Option<Arc<Mutex<mlog::MlogWriter>>>,
    ) -> Result<(), SessionError> {
        let track_alias = stream_header.subgroup_header.as_ref().unwrap().track_alias;
        log::trace!(
            "[SUBSCRIBER] recv_stream_inner: processing stream for track_alias={}",
            track_alias
        );

        // This is super silly, but I couldn't figure out a way to avoid the mutex guard across awaits.
        enum Writer {
            //Fetch(serve::FetchWriter),
            Subgroup(serve::SubgroupWriter),
        }

        let writer = {
            // Look up the subscribe id for this track alias
            if let Some(subscribe_id) = self.get_subscribe_id_by_alias(track_alias) {
                // Look up the subscribe by id
                let mut subscribes = self.subscribes.lock().unwrap();
                let subscribe = subscribes.get_mut(&subscribe_id).ok_or_else(|| {
                    log::error!(
                        "[SUBSCRIBER] recv_stream_inner: subscribe_id={} not found, track_alias={}",
                        subscribe_id,
                        track_alias
                    );
                    ServeError::NotFound
                })?;

                // Create the appropriate writer based on the stream header type
                if stream_header.header_type.is_subgroup() {
                    log::trace!("[SUBSCRIBER] recv_stream_inner: creating subgroup writer");
                    Writer::Subgroup(subscribe.subgroup(stream_header.subgroup_header.unwrap())?)
                } else {
                    log::error!(
                        "[SUBSCRIBER] recv_stream_inner: stream header_type={} not supported",
                        stream_header.header_type
                    );
                    return Err(SessionError::Serve(ServeError::Internal(format!(
                        "unsupported stream header type={}",
                        stream_header.header_type
                    ))));
                }
            } else {
                log::error!(
                    "[SUBSCRIBER] recv_stream_inner: subscription track_alias={} not found",
                    track_alias
                );
                return Err(SessionError::Serve(ServeError::NotFound));
            }
        };

        // Handle the stream based on the writer type
        match writer {
            //Writer::Fetch(fetch) => Self::recv_fetch(fetch, reader).await?,
            Writer::Subgroup(subgroup_writer) => {
                log::trace!("[SUBSCRIBER] recv_stream_inner: receiving subgroup data");
                Self::recv_subgroup(stream_header.header_type, subgroup_writer, reader, mlog)
                    .await?
            }
        };

        log::debug!(
            "[SUBSCRIBER] recv_stream_inner: completed processing stream for track_alias={}",
            track_alias
        );
        Ok(())
    }

    /// If new stream is a Subgroup stream, handle reception of subgroup objects and payloads.
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
            let (mut remaining_bytes, object_id_delta, status, decoded_object) =
                match stream_header_type.has_extension_headers() {
                    true => {
                        let object = reader.decode::<data::SubgroupObjectExt>().await?;
                        log::debug!(
                        "[SUBSCRIBER] recv_subgroup: object #{} with extension headers - object_id_delta={}, payload_length={}, status={:?}, extension_headers={:?}",
                        object_count + 1,
                        object.object_id_delta,
                        object.payload_length,
                        object.status,
                        object.extension_headers
                    );

                        // Check for known draft-14 extension types

                        // Check for Immutable Extensions (type 0xB = 11)
                        if object.extension_headers.has(0xB) {
                            log::warn!(
                                "[SUBSCRIBER] recv_subgroup: object #{} contains IMMUTABLE EXTENSIONS (type 0xB) - currently not forwarded/processed",
                                object_count + 1
                            );
                            if let Some(immutable_ext) = object.extension_headers.get(0xB) {
                                log::info!(
                                    "[SUBSCRIBER] recv_subgroup: immutable extension details: {:?}",
                                    immutable_ext
                                );
                            }
                        }

                        // Check for Prior Group ID Gap (type 0x3C = 60)
                        if object.extension_headers.has(0x3C) {
                            log::info!(
                                "[SUBSCRIBER] recv_subgroup: object #{} contains PRIOR GROUP ID GAP (type 0x3C)",
                                object_count + 1
                            );
                            if let Some(gap_ext) = object.extension_headers.get(0x3C) {
                                log::debug!(
                                    "[SUBSCRIBER] recv_subgroup: prior group id gap details: {:?}",
                                    gap_ext
                                );
                            }
                        }

                        let obj_copy = object.clone();
                        (
                            object.payload_length,
                            object.object_id_delta,
                            object.status,
                            Some(obj_copy),
                        )
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
                        (
                            object.payload_length,
                            object.object_id_delta,
                            object.status,
                            None,
                        )
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

    /// Handle reception of a datagram from the QUIC session.
    pub fn recv_datagram(&mut self, datagram: bytes::Bytes) -> Result<(), SessionError> {
        let mut cursor = io::Cursor::new(datagram);
        let datagram = data::Datagram::decode(&mut cursor)?;

        if let Some(ref mlog) = self.mlog {
            if let Ok(mut mlog_guard) = mlog.lock() {
                let time = mlog_guard.elapsed_ms();
                let stream_id = 0; // TODO: Placeholder, need actual QUIC stream ID
                let _ =
                    mlog_guard.add_event(mlog::object_datagram_parsed(time, stream_id, &datagram));
            }
        }

        // Check for extension headers in the datagram
        if let Some(ref ext_headers) = datagram.extension_headers {
            log::debug!(
                "[SUBSCRIBER] recv_datagram: datagram contains extension headers: {:?}",
                ext_headers
            );

            // Check for known draft-14 extension types

            // Check for Immutable Extensions (type 0xB = 11)
            if ext_headers.has(0xB) {
                log::warn!(
                    "[SUBSCRIBER] recv_datagram: datagram contains IMMUTABLE EXTENSIONS (type 0xB) - currently not forwarded/processed"
                );
                if let Some(immutable_ext) = ext_headers.get(0xB) {
                    log::info!(
                        "[SUBSCRIBER] recv_datagram: immutable extension details: {:?}",
                        immutable_ext
                    );
                }
            }

            // Check for Prior Group ID Gap (type 0x3C = 60)
            if ext_headers.has(0x3C) {
                log::info!(
                    "[SUBSCRIBER] recv_datagram: datagram contains PRIOR GROUP ID GAP (type 0x3C)"
                );
                if let Some(gap_ext) = ext_headers.get(0x3C) {
                    log::debug!(
                        "[SUBSCRIBER] recv_datagram: prior group id gap details: {:?}",
                        gap_ext
                    );
                }
            }
        }

        // Look up the subscribe id for this track alias
        if let Some(subscribe_id) = self.get_subscribe_id_by_alias(datagram.track_alias) {
            // Look up the subscribe by id
            if let Some(subscribe) = self.subscribes.lock().unwrap().get_mut(&subscribe_id) {
                log::trace!(
                    "[SUBSCRIBER] recv_datagram: track_alias={}, group_id={}, object_id={}, publisher_priority={}, status={}, payload_length={}",
                    datagram.track_alias,
                    datagram.group_id,
                    datagram.object_id.unwrap_or(0),
                    datagram.publisher_priority,
                    datagram.status.as_ref().map_or("None".to_string(), |s| format!("{:?}", s)),
                    datagram.payload.as_ref().map_or(0, |p| p.len()));
                subscribe.datagram(datagram)?;
            }
        } else {
            log::warn!(
                "[SUBSCRIBER] recv_datagram: discarded due to unknown track_alias: track_alias={}, group_id={}, object_id={}, publisher_priority={}, status={}, payload_length={}",
                datagram.track_alias,
                datagram.group_id,
                datagram.object_id.unwrap_or(0),
                datagram.publisher_priority,
                datagram.status.as_ref().map_or("None".to_string(), |s| format!("{:?}", s)),
                datagram.payload.as_ref().map_or(0, |p| p.len()));
        }

        Ok(())
    }
}
