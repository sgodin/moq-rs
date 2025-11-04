use std::collections::hash_map;
use std::collections::HashMap;

use std::sync::{Arc, Mutex};

use moq_transport::{
    coding::TrackNamespace,
    serve::{ServeError, TracksReader},
};

/// Registry of local tracks
#[derive(Clone)]
pub struct Locals {
    lookup: Arc<Mutex<HashMap<TrackNamespace, TracksReader>>>,
}

impl Default for Locals {
    fn default() -> Self {
        Self::new()
    }
}

/// Local tracks registry.
impl Locals {
    pub fn new() -> Self {
        Self {
            lookup: Default::default(),
        }
    }

    /// Register new local tracks.
    pub async fn register(&mut self, tracks: TracksReader) -> anyhow::Result<Registration> {
        let namespace = tracks.namespace.clone();

        // Insert the tracks(TracksReader) into the lookup table
        match self.lookup.lock().unwrap().entry(namespace.clone()) {
            hash_map::Entry::Vacant(entry) => entry.insert(tracks),
            hash_map::Entry::Occupied(_) => return Err(ServeError::Duplicate.into()),
        };

        let registration = Registration {
            locals: self.clone(),
            namespace,
        };

        Ok(registration)
    }

    /// Lookup local tracks by namespace.
    pub fn route(&self, namespace: &TrackNamespace) -> Option<TracksReader> {
        self.lookup.lock().unwrap().get(namespace).cloned()
    }
}

pub struct Registration {
    locals: Locals,
    namespace: TrackNamespace,
}

/// Deregister local tracks on drop.
impl Drop for Registration {
    fn drop(&mut self) {
        self.locals.lookup.lock().unwrap().remove(&self.namespace);
    }
}
