use super::{Publisher, SessionError};
use crate::message;

pub struct TrackStatusRequested {
    publisher: Publisher,
    pub request_msg: message::TrackStatus,
}

impl TrackStatusRequested {
    pub fn new(publisher: Publisher, request_msg: message::TrackStatus) -> Self {
        Self {
            publisher,
            request_msg,
        }
    }

    pub async fn respond(&mut self, status: message::TrackStatusOk) -> Result<(), SessionError> {
        self.publisher.send_message(status);
        Ok(())
    }
}
