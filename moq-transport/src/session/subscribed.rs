use std::ops;

use futures::stream::FuturesUnordered;
use futures::StreamExt;

use crate::coding::{Encode, Location, ReasonPhrase};
use crate::serve::{ServeError, TrackReaderMode};
use crate::watch::State;
use crate::{data, message, serve};

use super::{Publisher, SessionError, SubscribeInfo, Writer};

// This file defines Publisher handling of inbound Subscriptions

#[derive(Debug)]
struct SubscribedState {
    largest_location: Option<Location>,
    closed: Result<(), ServeError>,
}

impl SubscribedState {
    fn update_largest_location(&mut self, group_id: u64, object_id: u64) -> Result<(), ServeError> {
        if let Some(current_largest_location) = self.largest_location {
            let update_largest_location = Location::new(group_id, object_id);
            if update_largest_location > current_largest_location {
                self.largest_location = Some(update_largest_location);
            }
        }

        Ok(())
    }
}

impl Default for SubscribedState {
    fn default() -> Self {
        Self {
            largest_location: None,
            closed: Ok(()),
        }
    }
}

pub struct Subscribed {
    /// The sessions Publisher manager, used to send control messages,
    /// create new QUIC streams, and send datagrams
    publisher: Publisher,

    /// The Subscribe request message that created this subscription
    msg: message::Subscribe,

    /// The tracknamespace and trackname for the subscription.
    /// TODO SLG - is this needed? we have this information in the stored Subscribe
    ///            message.
    pub info: SubscribeInfo,

    state: State<SubscribedState>,

    /// Tracks if SubscribeOk has been sent yet or not. Used to send
    /// SubscribeDone vs SubscribeError on drop.
    ok: bool,
}

impl Subscribed {
    pub(super) fn new(publisher: Publisher, msg: message::Subscribe) -> (Self, SubscribedRecv) {
        let (send, recv) = State::default().split();
        let info = SubscribeInfo {
            namespace: msg.track_namespace.clone(),
            name: msg.track_name.clone(),
        };

        let send = Self {
            publisher,
            state: send,
            msg,
            info,
            ok: false,
        };

        // Prevents updates after being closed
        let recv = SubscribedRecv { state: recv };

        (send, recv)
    }

    pub async fn serve(mut self, track: serve::TrackReader) -> Result<(), SessionError> {
        let res = self.serve_inner(track).await;
        if let Err(err) = &res {
            self.close(err.clone().into())?;
        }

        res
    }

    async fn serve_inner(&mut self, track: serve::TrackReader) -> Result<(), SessionError> {
        let latest = track.latest();
        self.state
            .lock_mut()
            .ok_or(ServeError::Cancel)?
            .largest_location = latest;

        self.publisher.send_message(message::SubscribeOk {
            id: self.msg.id,
            track_alias: self.msg.id, // TODO SLG - use subscription id for now, needs fixing
            expires: 3600,            // TODO SLG
            group_order: message::GroupOrder::Descending, // TODO: resolve correct value from publisher / subscriber prefs
            content_exists: latest.is_some(),
            largest_location: latest,
            params: Default::default(),
        });

        self.ok = true; // So we send SubscribeDone on drop

        match track.mode().await? {
            // TODO cancel track/datagrams on closed
            TrackReaderMode::Stream(_stream) => panic!("deprecated"),
            TrackReaderMode::Subgroups(subgroups) => self.serve_subgroups(subgroups).await,
            TrackReaderMode::Datagrams(datagrams) => self.serve_datagrams(datagrams).await,
        }
    }

    pub fn close(self, err: ServeError) -> Result<(), ServeError> {
        let state = self.state.lock();
        state.closed.clone()?;

        let mut state = state.into_mut().ok_or(ServeError::Done)?;
        state.closed = Err(err);

        Ok(())
    }

    pub async fn closed(&self) -> Result<(), ServeError> {
        loop {
            {
                let state = self.state.lock();
                state.closed.clone()?;

                match state.modified() {
                    Some(notify) => notify,
                    None => return Ok(()),
                }
            }
            .await;
        }
    }
}

impl ops::Deref for Subscribed {
    type Target = SubscribeInfo;

    fn deref(&self) -> &Self::Target {
        &self.info
    }
}

impl Drop for Subscribed {
    fn drop(&mut self) {
        let state = self.state.lock();
        let err = state
            .closed
            .as_ref()
            .err()
            .cloned()
            .unwrap_or(ServeError::Done);
        drop(state); // Important to avoid a deadlock

        if self.ok {
            self.publisher.send_message(message::PublishDone {
                id: self.msg.id,
                status_code: err.code(),
                stream_count: 0, // TODO SLG
                reason: ReasonPhrase(err.to_string()),
            });
        } else {
            self.publisher.send_message(message::SubscribeError {
                id: self.msg.id,
                error_code: err.code(),
                reason_phrase: ReasonPhrase(err.to_string()),
            });
        };
    }
}

impl Subscribed {
    async fn serve_subgroups(
        &mut self,
        mut subgroups: serve::SubgroupsReader,
    ) -> Result<(), SessionError> {
        let mut tasks = FuturesUnordered::new();
        let mut done: Option<Result<(), ServeError>> = None;

        loop {
            tokio::select! {
                res = subgroups.next(), if done.is_none() => match res {
                    Ok(Some(subgroup)) => {
                        let header = data::SubgroupHeader {
                            header_type: data::StreamHeaderType::SubgroupIdEndOfGroup,  // SubGroupId = Yes, Extensions = No, ContainsEndOfGroup = yes
                            track_alias: self.msg.id, // TODO SLG - use subscription id for now, needs fixing
                            group_id: subgroup.group_id,
                            subgroup_id: Some(subgroup.subgroup_id),
                            publisher_priority: subgroup.priority,
                        };

                        let publisher = self.publisher.clone();
                        let state = self.state.clone();
                        let info = subgroup.info.clone();

                        tasks.push(async move {
                            if let Err(err) = Self::serve_subgroup(header, subgroup, publisher, state).await {
                                log::warn!("failed to serve subgroup: {:?}, error: {}", info, err);
                            }
                        });
                    },
                    Ok(None) => done = Some(Ok(())),
                    Err(err) => done = Some(Err(err)),
                },
                res = self.closed(), if done.is_none() => done = Some(res),
                _ = tasks.next(), if !tasks.is_empty() => {},
                else => return Ok(done.unwrap()?),
            }
        }
    }

    async fn serve_subgroup(
        header: data::SubgroupHeader,
        mut subgroup_reader: serve::SubgroupReader,
        mut publisher: Publisher,
        state: State<SubscribedState>,
    ) -> Result<(), SessionError> {
        let mut send_stream = publisher.open_uni().await?;

        // TODO figure out u32 vs u64 priority
        send_stream.set_priority(subgroup_reader.priority as i32);

        let mut writer = Writer::new(send_stream);

        log::trace!("sending subgroup header: {:?}", header);

        writer.encode(&header).await?;

        while let Some(mut subgroup_object_reader) = subgroup_reader.next().await? {
            let subgroup_object = data::SubgroupObject {
                object_id_delta: 0, // before delta logic, used to be subgroup_object_reader.object_id,
                payload_length: subgroup_object_reader.size,
                status: if subgroup_object_reader.size == 0 {
                    // Only set status if payload length is zero
                    Some(subgroup_object_reader.status)
                } else {
                    None
                },
            };

            writer.encode(&subgroup_object).await?;

            state
                .lock_mut()
                .ok_or(ServeError::Done)?
                .update_largest_location(
                    subgroup_reader.group_id,
                    subgroup_object_reader.object_id,
                )?;

            log::trace!("sent subgroup object: {:?}", subgroup_object);

            while let Some(chunk) = subgroup_object_reader.read().await? {
                writer.write(&chunk).await?;
                // payload length already logged when subgroup object is logged
                //log::trace!("sent group payload len: {:?}", chunk.len());
            }

            //log::trace!("sent subgroup done");
        }

        Ok(())
    }

    async fn serve_datagrams(
        &mut self,
        mut datagrams: serve::DatagramsReader,
    ) -> Result<(), SessionError> {
        while let Some(datagram) = datagrams.read().await? {
            let datagram = data::Datagram {
                datagram_type: data::DatagramType::ObjectIdPayload, // TODO SLG
                track_alias: self.msg.id, //  TODO SLG - use subscription id for now
                group_id: datagram.group_id,
                object_id: Some(datagram.object_id),
                publisher_priority: datagram.priority,
                extension_headers: None,
                status: None,
                payload: Some(datagram.payload),
            };

            let mut buffer =
                bytes::BytesMut::with_capacity(datagram.payload.as_ref().unwrap().len() + 100);
            datagram.encode(&mut buffer)?;

            self.publisher.send_datagram(buffer.into()).await?;
            log::trace!("sent datagram: {:?}", datagram);

            self.state
                .lock_mut()
                .ok_or(ServeError::Done)?
                .update_largest_location(datagram.group_id, datagram.object_id.unwrap())?;
            // TODO SLG - fix up safety of unwrap()
        }

        Ok(())
    }
}

pub(super) struct SubscribedRecv {
    state: State<SubscribedState>,
}

impl SubscribedRecv {
    pub fn recv_unsubscribe(&mut self) -> Result<(), ServeError> {
        let state = self.state.lock();
        state.closed.clone()?;

        if let Some(mut state) = state.into_mut() {
            state.closed = Err(ServeError::Cancel);
        }

        Ok(())
    }
}
