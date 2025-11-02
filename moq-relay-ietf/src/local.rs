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

    /// Lookup local tracks by namespace using hierarchical prefix matching.
    /// Returns the TracksReader for the longest matching namespace prefix.
    pub fn route(&self, namespace: &TrackNamespace) -> Option<TracksReader> {
        let lookup = self.lookup.lock().unwrap();
        
        // Find the longest matching prefix
        let mut best_match: Option<(usize, TracksReader)> = None;
        
        for (registered_ns, tracks) in lookup.iter() {
            if registered_ns.is_prefix_of(namespace) {
                let prefix_len = registered_ns.fields.len();
                if best_match.is_none() || best_match.as_ref().unwrap().0 < prefix_len {
                    best_match = Some((prefix_len, tracks.clone()));
                }
            }
        }
        
        best_match.map(|(_, tracks)| tracks)
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

#[cfg(test)]
mod tests {
    use super::*;
    use moq_transport::{coding::TrackNamespace, serve::Tracks};

    #[tokio::test]
    async fn test_exact_match() {
        let mut locals = Locals::new();
        let namespace = TrackNamespace::from_utf8_path("moq-test-00");
        let (_writer, _request, reader) = Tracks::new(namespace.clone()).produce();

        let _registration = locals.register(reader).await.unwrap();

        // Exact match should work
        let result = locals.route(&namespace);
        assert!(result.is_some());
    }

    #[tokio::test]
    async fn test_hierarchical_match() {
        let mut locals = Locals::new();
        let registered_ns = TrackNamespace::from_utf8_path("moq-test-00");
        let (_writer, _request, reader) = Tracks::new(registered_ns.clone()).produce();

        let _registration = locals.register(reader).await.unwrap();

        // Hierarchical match should work
        let query_ns = TrackNamespace::from_utf8_path("moq-test-00/1/2/3");
        let result = locals.route(&query_ns);
        assert!(
            result.is_some(),
            "Should match hierarchical namespace moq-test-00/1/2/3 to registered moq-test-00"
        );
    }

    #[tokio::test]
    async fn test_no_match() {
        let mut locals = Locals::new();
        let registered_ns = TrackNamespace::from_utf8_path("moq-test-00");
        let (_writer, _request, reader) = Tracks::new(registered_ns.clone()).produce();

        let _registration = locals.register(reader).await.unwrap();

        // Different namespace should not match
        let query_ns = TrackNamespace::from_utf8_path("other-namespace");
        let result = locals.route(&query_ns);
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_longest_match_wins() {
        let mut locals = Locals::new();

        // Register two namespaces at different levels
        let ns1 = TrackNamespace::from_utf8_path("moq-test-00");
        let (_writer1, _request1, reader1) = Tracks::new(ns1.clone()).produce();
        let _reg1 = locals.register(reader1).await.unwrap();

        let ns2 = TrackNamespace::from_utf8_path("moq-test-00/1");
        let (_writer2, _request2, reader2) = Tracks::new(ns2.clone()).produce();
        let _reg2 = locals.register(reader2).await.unwrap();

        // Query for a deeper namespace
        let query_ns = TrackNamespace::from_utf8_path("moq-test-00/1/2/3");
        let result = locals.route(&query_ns);
        assert!(result.is_some());

        // The result should be the more specific match (moq-test-00/1)
        // We can verify this by checking the namespace on the returned TracksReader
        let tracks_reader = result.unwrap();
        assert_eq!(tracks_reader.namespace, ns2);
    }

    #[tokio::test]
    async fn test_partial_match_fails() {
        let mut locals = Locals::new();
        let registered_ns = TrackNamespace::from_utf8_path("moq-test-00/1/2");
        let (_writer, _request, reader) = Tracks::new(registered_ns.clone()).produce();

        let _registration = locals.register(reader).await.unwrap();

        // Querying for a shorter namespace should not match
        let query_ns = TrackNamespace::from_utf8_path("moq-test-00/1");
        let result = locals.route(&query_ns);
        assert!(result.is_none(), "Shorter namespace should not match longer registered namespace");
    }
}
