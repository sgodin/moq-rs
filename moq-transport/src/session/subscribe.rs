use std::ops;

use crate::{
    coding::TrackNamespace,
    data,
    message::{self, FilterType, GroupOrder},
    serve::{self, ServeError, TrackWriter, TrackWriterMode},
};

use crate::watch::State;

use super::Subscriber;

#[derive(Debug, Clone)]
pub struct SubscribeInfo {
    pub namespace: TrackNamespace,
    pub name: String,
}

struct SubscribeState {
    ok: bool,
    closed: Result<(), ServeError>,
}

impl Default for SubscribeState {
    fn default() -> Self {
        Self {
            ok: Default::default(),
            closed: Ok(()),
        }
    }
}

// Held by the application
#[must_use = "unsubscribe on drop"]
pub struct Subscribe {
    state: State<SubscribeState>,
    subscriber: Subscriber,
    id: u64,

    pub info: SubscribeInfo,
}

impl Subscribe {
    pub(super) fn new(
        mut subscriber: Subscriber,
        id: u64,
        track: TrackWriter,
    ) -> (Subscribe, SubscribeRecv) {
        subscriber.send_message(message::Subscribe {
            id,
            track_namespace: track.namespace.clone(),
            track_name: track.name.clone(),
            // TODO add prioritization logic on the publisher side
            subscriber_priority: 127, // default to mid value, see: https://github.com/moq-wg/moq-transport/issues/504
            group_order: GroupOrder::Publisher, // defer to publisher send order
            forward: true,  // default to forwarding objects
            filter_type: FilterType::NextGroupStart,
            start_location: None,
            end_group_id: None,
            params: Default::default(),
        });

        let info = SubscribeInfo {
            namespace: track.namespace.clone(),
            name: track.name.clone(),
        };

        let (send, recv) = State::default().split();

        let send = Subscribe {
            state: send,
            subscriber,
            id,
            info,
        };

        let recv = SubscribeRecv {
            state: recv,
            writer: Some(track.into()),
        };

        (send, recv)
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

impl Drop for Subscribe {
    fn drop(&mut self) {
        self.subscriber
            .send_message(message::Unsubscribe { id: self.id });
    }
}

impl ops::Deref for Subscribe {
    type Target = SubscribeInfo;

    fn deref(&self) -> &SubscribeInfo {
        &self.info
    }
}

pub(super) struct SubscribeRecv {
    state: State<SubscribeState>,
    writer: Option<TrackWriterMode>,
}

impl SubscribeRecv {
    pub fn ok(&mut self) -> Result<(), ServeError> {
        let state = self.state.lock();
        if state.ok {
            return Err(ServeError::Duplicate);
        }

        if let Some(mut state) = state.into_mut() {
            state.ok = true;
        }

        Ok(())
    }

    pub fn error(mut self, err: ServeError) -> Result<(), ServeError> {
        if let Some(writer) = self.writer.take() {
            writer.close(err.clone())?;
        }

        let state = self.state.lock();
        state.closed.clone()?;

        let mut state = state.into_mut().ok_or(ServeError::Cancel)?;
        state.closed = Err(err);

        Ok(())
    }

    pub fn track(&mut self, header: data::TrackHeader) -> Result<serve::StreamWriter, ServeError> {
        let writer = self.writer.take().ok_or(ServeError::Done)?;

        let stream = match writer {
            TrackWriterMode::Track(init) => init.stream(header.publisher_priority)?,
            _ => return Err(ServeError::Mode),
        };

        self.writer = Some(stream.clone().into());

        Ok(stream)
    }

    pub fn subgroup(
        &mut self,
        header: data::SubgroupHeader,
    ) -> Result<serve::SubgroupWriter, ServeError> {
        let writer = self.writer.take().ok_or(ServeError::Done)?;

        let mut subgroups = match writer {
            TrackWriterMode::Track(init) => init.groups()?,
            TrackWriterMode::Subgroups(subgroups) => subgroups,
            _ => return Err(ServeError::Mode),
        };

        let writer = subgroups.create(serve::Subgroup {
            group_id: header.group_id,
            subgroup_id: header.subgroup_id.unwrap(),  // TODO SLG - supgroup_id may not be present
            priority: header.publisher_priority,
        })?;

        self.writer = Some(subgroups.into());

        Ok(writer)
    }

    pub fn datagram(&mut self, datagram: data::Datagram) -> Result<(), ServeError> {
        let writer = self.writer.take().ok_or(ServeError::Done)?;

        let mut datagrams = match writer {
            TrackWriterMode::Track(init) => init.datagrams()?,
            TrackWriterMode::Datagrams(datagrams) => datagrams,
            _ => return Err(ServeError::Mode),
        };

        // TODO SLG - update with new datagram fields
        datagrams.write(serve::Datagram {
            group_id: datagram.group_id,
            object_id: datagram.object_id,
            priority: datagram.publisher_priority,
            payload: datagram.payload.unwrap(),  // TODO SLG - datagram.payload is an Option
        })?;

        Ok(())
    }
}
