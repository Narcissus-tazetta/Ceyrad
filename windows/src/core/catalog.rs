use std::collections::VecDeque;
use std::sync::mpsc::Receiver;

use super::models::{CatalogInfo, TrackInfo};
use super::source::Source;

#[derive(Debug, Clone)]
pub struct Request {
    /// `TrackInfo::identity`, so a late answer can be matched against whatever
    /// is playing by the time it lands.
    pub key: String,
    pub name: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: Option<f64>,
    /// YouTube Music's spelling does not follow the machine's region, so its
    /// iTunes fallback also searches the US and JP storefronts.
    pub source: Source,
    /// The app's menu language (`en` / `ja`), a hint at the language the
    /// user's YouTube Music is in.
    pub app_language: &'static str,
}

/// What the lookup came back with. A miss and a failure look the same to the
/// user but must not be treated the same: a miss is the final answer for that
/// track, a failure is worth trying again.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Found(CatalogInfo),
    /// The catalog was searched and does not have this track.
    Missing,
    /// The lookup never completed — offline, timed out, or the API refused.
    Failed,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub source: Source,
    pub key: String,
    pub outcome: Outcome,
}

impl Resolved {
    pub fn matches(&self, source: Source, track: &TrackInfo) -> bool {
        self.source == source && track.matches_identity(&self.key)
    }
}

impl Outcome {
    pub fn from_cached(catalog: Option<CatalogInfo>) -> Self {
        match catalog {
            Some(catalog) => Outcome::Found(catalog),
            None => Outcome::Missing,
        }
    }
}

/// One waiting track per source: skipping in one player must not discard the other.
pub struct Queue {
    pending: VecDeque<Request>,
}

impl Default for Queue {
    fn default() -> Self {
        Self::new()
    }
}

impl Queue {
    pub fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn push(&mut self, request: Request) {
        self.pending.retain(|held| held.source != request.source);
        self.pending.push_back(request);
    }

    pub fn pop(&mut self) -> Option<Request> {
        self.pending.pop_front()
    }

    pub fn has_newer(&self, source: Source) -> bool {
        self.pending.iter().any(|request| request.source == source)
    }

    pub fn drain_from(&mut self, requests: &Receiver<Request>) {
        while let Ok(request) = requests.try_recv() {
            self.push(request);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn request(source: Source, name: &str) -> Request {
        Request {
            key: TrackInfo::new(name, "Artist", "Album").identity(),
            name: name.into(),
            artist: "Artist".into(),
            album: "Album".into(),
            duration_sec: None,
            source,
            app_language: "en",
        }
    }

    #[test]
    fn both_sources_wait_without_discarding_each_other() {
        let mut queue = Queue::new();
        queue.push(request(Source::AppleMusic, "Apple"));
        queue.push(request(Source::YouTubeMusic, "YouTube"));
        assert_eq!(queue.pop().unwrap().name, "Apple");
        assert_eq!(queue.pop().unwrap().name, "YouTube");
        assert!(queue.is_empty());
    }

    #[test]
    fn skipping_replaces_only_the_same_sources_pending_track() {
        let (send, receive) = channel();
        send.send(request(Source::AppleMusic, "Skipped")).unwrap();
        send.send(request(Source::YouTubeMusic, "YouTube")).unwrap();
        send.send(request(Source::AppleMusic, "Current")).unwrap();
        let mut queue = Queue::new();
        queue.drain_from(&receive);
        assert_eq!(queue.pop().unwrap().name, "YouTube");
        assert_eq!(queue.pop().unwrap().name, "Current");
        assert!(queue.is_empty());
    }

    #[test]
    fn other_sources_request_does_not_supersede_an_inflight_lookup() {
        let mut queue = Queue::new();
        queue.push(request(Source::AppleMusic, "Apple"));
        assert!(!queue.has_newer(Source::YouTubeMusic));
        assert!(queue.has_newer(Source::AppleMusic));
    }

    #[test]
    fn same_track_results_apply_only_to_the_requested_source() {
        let track = TrackInfo::new("Song", "Artist", "Album");
        for source in Source::ALL {
            let result = Resolved {
                source,
                key: track.identity(),
                outcome: Outcome::Found(CatalogInfo::default()),
            };
            for recipient in Source::ALL {
                assert_eq!(result.matches(recipient, &track), recipient == source);
            }
        }
    }

    #[test]
    fn late_result_does_not_apply_after_skipping() {
        let result = Resolved {
            source: Source::YouTubeMusic,
            key: TrackInfo::new("Old", "Artist", "Album").identity(),
            outcome: Outcome::Missing,
        };
        assert!(!result.matches(
            Source::YouTubeMusic,
            &TrackInfo::new("New", "Artist", "Album")
        ));
    }
}
