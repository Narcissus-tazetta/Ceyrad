//! Resolving a track to its Apple Music catalog entry — artwork and links.
//!
//! SMTC gives a title, an artist and a position; it does not give a URL, and
//! its thumbnail is a byte stream Discord has no way to accept. Discord's
//! `large_image` wants a URL, so the artwork has to come from somewhere that
//! serves one. The macOS build asks the iTunes Search API; so does this.
//!
//! The lookup runs on its own thread. It is the only blocking network call in
//! the app, and the event loop must stay responsive to SMTC and to Discord
//! while it is in flight — so the loop posts a request, carries on without
//! artwork, and picks the answer up on a later pass.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::app::log;
use crate::core::itunes;
use crate::core::models::{CatalogInfo, MusicSourceId};
use crate::discord::pipe::Event as WakeEvent;
use crate::winhttp::{ComApartment, Http};

/// The API is undocumented but rate limited at roughly 20 requests a minute,
/// so requests are spaced out rather than sent as fast as tracks change.
const MIN_REQUEST_INTERVAL: Duration = Duration::from_secs(3);

/// Misses are cached too: a locally imported track is in no catalog, and
/// without a negative entry it would be looked up again on every replay.
const CACHE_CAPACITY: usize = 300;

/// Storefront used when the machine's region cannot be read.
const FALLBACK_COUNTRY: &str = "US";

#[derive(Debug, Clone)]
pub struct Request {
    pub source: MusicSourceId,
    /// `TrackInfo::identity`, so a late answer can be matched against whatever
    /// is playing by the time it lands.
    pub key: String,
    pub name: String,
    pub artist: String,
    pub album: String,
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
    pub source: MusicSourceId,
    pub key: String,
    pub outcome: Outcome,
}

pub struct Resolver {
    requests: Sender<Request>,
    results: Receiver<Resolved>,
}

impl Resolver {
    /// Signals `wake` after posting a result, so the event loop leaves its wait
    /// and applies the artwork instead of sitting on it until the next song.
    pub fn new(wake: Arc<WakeEvent>) -> Self {
        let (requests, request_rx) = channel::<Request>();
        let (result_tx, results) = channel::<Resolved>();

        thread::Builder::new()
            .name("catalog".into())
            .spawn(move || worker(request_rx, result_tx, wake))
            .expect("failed to spawn the catalog thread");

        Self { requests, results }
    }

    pub fn request(&self, request: Request) {
        let _ = self.requests.send(request);
    }

    pub fn try_recv(&self) -> Option<Resolved> {
        self.results.try_recv().ok()
    }
}

fn worker(requests: Receiver<Request>, results: Sender<Resolved>, wake: Arc<WakeEvent>) {
    // WinRT apartments are per-thread, so this thread declares its own rather
    // than leaning on whatever the main thread happened to choose.
    let _com = ComApartment::enter();

    let country = storefront_country();
    let http = match Http::new() {
        Ok(http) => http,
        // Without an HTTP client there is nothing this thread can do. Said out
        // loud, because the symptom — artwork that never appears — otherwise
        // has no visible cause at all.
        Err(e) => {
            log(&format!(
                "catalog: no HTTP client ({e}); artwork and links are unavailable"
            ));
            return;
        }
    };

    let mut cache = Cache::new();
    let mut queue = Queue::new();
    let mut next_allowed = Instant::now();

    loop {
        // Nothing to do: park until the event loop asks for something.
        if queue.is_empty() {
            match requests.recv() {
                Ok(request) => queue.push(request),
                Err(_) => break,
            }
        }
        queue.drain_from(&requests);

        let Some(request) = queue.pop() else {
            continue;
        };
        if let Some(catalog) = cache.get(&request.key) {
            send(&results, &wake, &request, Outcome::from_cached(catalog));
            continue;
        }

        let wait = next_allowed.saturating_duration_since(Instant::now());
        if !wait.is_zero() {
            thread::sleep(wait);
            // Waiting is exactly when a newer track arrives, so look again
            // before spending the slot on one that is no longer playing.
            queue.drain_from(&requests);
            if queue.has_newer_for(request.source) {
                continue;
            }
            if let Some(catalog) = cache.get(&request.key) {
                send(&results, &wake, &request, Outcome::from_cached(catalog));
                continue;
            }
        }
        next_allowed = Instant::now() + MIN_REQUEST_INTERVAL;

        let url = itunes::search_url(&request.name, &request.artist, &country);
        let outcome = match http.get(&url) {
            Ok(body) => match itunes::parse_response(&body) {
                Some(response) => {
                    let found = itunes::pick_best(
                        &response.results,
                        &request.name,
                        &request.artist,
                        &request.album,
                    )
                    .map(itunes::catalog_from)
                    .and_then(|catalog| for_source(request.source, catalog));

                    // Only a real answer is cached: a temporary failure must
                    // not become this track's permanent one.
                    cache.store(request.key.clone(), found.clone());
                    Outcome::from_cached(found)
                }
                None => {
                    log("catalog: the search API returned something unparseable");
                    Outcome::Failed
                }
            },
            Err(e) => {
                log(&format!("catalog: lookup failed ({e})"));
                Outcome::Failed
            }
        };
        send(&results, &wake, &request, outcome);
    }

    // Every sender is gone, which only happens as the app shuts down.
}

fn send(results: &Sender<Resolved>, wake: &WakeEvent, request: &Request, outcome: Outcome) {
    let _ = results.send(Resolved {
        source: request.source,
        key: request.key.clone(),
        outcome,
    });
    let _ = wake.set();
}

impl Outcome {
    fn from_cached(catalog: Option<CatalogInfo>) -> Self {
        match catalog {
            Some(catalog) => Outcome::Found(catalog),
            None => Outcome::Missing,
        }
    }
}

/// What a source is allowed to take from an Apple Music catalog entry.
///
/// The links only make sense under the Apple Music presence. Spotify's buttons
/// say "Play on Spotify"; pointing them at `music.apple.com` would be a lie, so
/// only the cover art — which is the same record either way — crosses over.
fn for_source(source: MusicSourceId, catalog: CatalogInfo) -> Option<CatalogInfo> {
    match source {
        MusicSourceId::AppleMusic => Some(catalog),
        MusicSourceId::Spotify => itunes::artwork_only(catalog),
    }
}

/// At most one outstanding request per source.
///
/// A burst of skips leaves several requests queued and only the last is still
/// on screen, so the rest are dropped without spending a rate-limit slot. Doing
/// that per source rather than globally is what stops one player's lookup from
/// swallowing the other's when both are playing.
struct Queue {
    pending: VecDeque<Request>,
}

impl Queue {
    fn new() -> Self {
        Self {
            pending: VecDeque::new(),
        }
    }

    fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    fn push(&mut self, request: Request) {
        match self
            .pending
            .iter_mut()
            .find(|queued| queued.source == request.source)
        {
            Some(queued) => *queued = request,
            None => self.pending.push_back(request),
        }
    }

    fn pop(&mut self) -> Option<Request> {
        self.pending.pop_front()
    }

    /// Whether a newer request for this source arrived while we were waiting,
    /// which makes the one in hand not worth a lookup.
    fn has_newer_for(&self, source: MusicSourceId) -> bool {
        self.pending.iter().any(|queued| queued.source == source)
    }

    /// Takes everything already queued without blocking. A disconnected
    /// channel needs no special case: the queue drains, and the blocking
    /// `recv` at the top of the worker loop ends it.
    fn drain_from(&mut self, requests: &Receiver<Request>) {
        while let Ok(request) = requests.try_recv() {
            self.push(request);
        }
    }
}

struct Cache {
    entries: HashMap<String, Option<CatalogInfo>>,
    order: VecDeque<String>,
}

impl Cache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// `Some(catalog)` for a hit — including `Some(None)` for a cached miss —
    /// and `None` when the track has never been looked up.
    fn get(&self, key: &str) -> Option<Option<CatalogInfo>> {
        self.entries.get(key).cloned()
    }

    fn store(&mut self, key: String, catalog: Option<CatalogInfo>) {
        if !self.entries.contains_key(&key) {
            self.order.push_back(key.clone());
        }
        self.entries.insert(key, catalog);
        while self.order.len() > CACHE_CAPACITY {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }
}

/// The machine's region, which picks the storefront the search runs against —
/// a Japanese user should get `music.apple.com/jp` links, not US ones.
fn storefront_country() -> String {
    use windows::Win32::Globalization::GetUserDefaultGeoName;

    let mut buffer = [0u16; 16];
    let written = unsafe { GetUserDefaultGeoName(&mut buffer) };
    if written <= 1 {
        return FALLBACK_COUNTRY.to_string();
    }
    let name = String::from_utf16_lossy(&buffer[..(written - 1) as usize]);
    // The API wants a two-letter storefront; `GetUserDefaultGeoName` can also
    // return things like `001` (world) or a longer tag.
    if name.len() == 2 && name.chars().all(|c| c.is_ascii_alphabetic()) {
        name.to_ascii_uppercase()
    } else {
        FALLBACK_COUNTRY.to_string()
    }
}
