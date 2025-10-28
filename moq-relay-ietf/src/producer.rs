use futures::{stream::FuturesUnordered, StreamExt};
use moq_transport::{
    serve::{ServeError, TracksReader},
    session::{Publisher, SessionError, Subscribed},
};

use crate::{Locals, RemotesConsumer};

/// Producer of tracks to a remote server.
#[derive(Clone)]
pub struct Producer {
    remote_publisher: Publisher,
    locals: Locals,
    remotes: Option<RemotesConsumer>,
}

impl Producer {
    pub fn new(remote: Publisher, locals: Locals, remotes: Option<RemotesConsumer>) -> Self {
        Self {
            remote_publisher: remote,
            locals,
            remotes,
        }
    }

    /// Announce new tracks to the remote server.
    pub async fn announce(&mut self, tracks: TracksReader) -> Result<(), SessionError> {
        self.remote_publisher.announce(tracks).await
    }

    /// Run the producer to serve subscribe requests.
    pub async fn run(mut self) -> Result<(), SessionError> {
        let mut tasks = FuturesUnordered::new();

        loop {
            tokio::select! {
                // Handle a new subscribe request
                Some(subscribe) = self.remote_publisher.subscribed() => {
                    let this = self.clone();

                    // Spawn a new task to handle the subscribe
                    tasks.push(async move {
                        let info = subscribe.clone();
                        log::info!("serving subscribe: {:?}", info);

                        // Serve the subscribe request
                        if let Err(err) = this.serve(subscribe).await {
                            log::warn!("failed serving subscribe: {:?}, error: {}", info, err)
                        }
                    })
                },
                _= tasks.next(), if !tasks.is_empty() => {},
                else => return Ok(()),
            };
        }
    }

    /// Serve a subscribe request.
    async fn serve(self, subscribe: Subscribed) -> Result<(), anyhow::Error> {
        // Check local tracks first, and serve from local if possible
        if let Some(mut local) = self.locals.route(&subscribe.namespace) {
            if let Some(track) = local.subscribe(&subscribe.name) {
                log::info!("serving from local: {:?}", track.info);
                return Ok(subscribe.serve(track).await?);
            }
        }

        // Check remote tracks second, and serve from remote if possible
        if let Some(remotes) = &self.remotes {
            // Try to route to a remote for this namespace
            if let Some(remote) = remotes.route(&subscribe.namespace).await? {
                if let Some(track) =
                    remote.subscribe(subscribe.namespace.clone(), subscribe.name.clone())?
                {
                    log::info!("serving from remote: {:?} {:?}", remote.info, track.info);

                    // NOTE: Depends on drop(track) being called afterwards
                    return Ok(subscribe.serve(track.reader).await?);
                }
            }
        }

        Err(ServeError::NotFound.into())
    }
}
