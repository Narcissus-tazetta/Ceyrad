//! Blocking HTTP over WinRT's stack, plus the COM apartment a caller thread
//! needs to use it.
//!
//! WinRT is already in the process for SMTC, so reaching the network this way
//! costs no extra dependency. What it does not give us is a deadline, which is
//! what `crate::winrt::join_with_timeout` is for.
//!
//! Shared by `catalog`, whose thread lives as long as the app, and `updater`,
//! which spawns one per check — both of them doing nothing but blocking on one
//! of these.

use std::time::{Duration, Instant};

use windows::core::{Error as WinError, Result as WinResult, HRESULT, HSTRING};
use windows::Foundation::Uri;
use windows::Storage::Streams::{Buffer, DataReader, IInputStream, InputStreamOptions};
use windows::Web::Http::{
    HttpClient, HttpCompletionOption, HttpMethod, HttpRequestMessage, IHttpContent,
};
use windows::Win32::Foundation::{ERROR_FILE_TOO_LARGE, ERROR_INVALID_DATA};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

use crate::winrt::join_with_timeout;

/// Ceiling on one request, matching the macOS build's `timeoutIntervalForRequest`.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Ceiling on request plus body, matching `timeoutIntervalForResource`.
pub const RESOURCE_TIMEOUT: Duration = Duration::from_secs(15);

/// Ceiling on a response body. Without one the only bound on how much a remote
/// server can make this process allocate is the resource timeout, which on a
/// fast link is hundreds of megabytes — the whole memory ceiling of a tray app
/// set by someone else. 256 KB clears both a 10-result iTunes response and a
/// GitHub release object with room to spare.
const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

/// Holds this thread's COM apartment for as long as it is alive.
///
/// WinRT apartments are per-thread. A worker that skips this survives only on
/// whatever the process happens to have already, which is a dependency on the
/// main thread's choice that would break the moment that choice changed.
pub struct ComApartment {
    /// Whether this object is the one that entered the apartment. A caller that
    /// got `RPC_E_CHANGED_MODE` never entered it, and calling `CoUninitialize`
    /// in that case decrements a count that belongs to someone else.
    entered: bool,
}

impl ComApartment {
    pub fn enter() -> Self {
        // MTA: a worker owns no window and pumps no messages, and every WinRT
        // call it makes is made from this one thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self {
            entered: hr.is_ok(),
        }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.entered {
            unsafe { CoUninitialize() };
        }
    }
}

pub struct Http {
    client: HttpClient,
}

impl Http {
    pub fn new() -> WinResult<Self> {
        Ok(Self {
            client: HttpClient::new()?,
        })
    }

    /// Blocks this thread until the response is complete, or until the timeouts
    /// run out. Safe only because nothing else runs on a worker thread — the
    /// event loop is elsewhere.
    pub fn get(&self, url: &str) -> WinResult<String> {
        self.get_with_user_agent(url, None)
    }

    /// As `get`, with a `User-Agent`. Some APIs — GitHub's among them — refuse
    /// a request that does not identify itself, so the header is not optional
    /// there even though HTTP says it is.
    pub fn get_with_user_agent(&self, url: &str, user_agent: Option<&str>) -> WinResult<String> {
        let deadline = Instant::now() + RESOURCE_TIMEOUT;
        let uri = Uri::CreateUri(&HSTRING::from(url))?;

        let request = HttpRequestMessage::Create(&HttpMethod::Get()?, &uri)?;
        if let Some(user_agent) = user_agent {
            request
                .Headers()?
                .Append(&HSTRING::from("User-Agent"), &HSTRING::from(user_agent))?;
        }

        // Headers first, so an oversized body can be refused before it is read
        // rather than after it is already in memory.
        let response = join_with_timeout(
            &self
                .client
                .SendRequestWithOptionAsync(&request, HttpCompletionOption::ResponseHeadersRead)?,
            REQUEST_TIMEOUT.min(remaining(deadline)),
        )?;
        // Not `EnsureSuccessStatusCode`, which raises an HRESULT and nothing
        // else. The status is the whole diagnosis here: the iTunes Search API
        // answers 403 when it decides a caller is asking too often, and in a
        // log that only says "lookup failed" that is indistinguishable from
        // being offline — the two want opposite fixes.
        if !response.IsSuccessStatusCode()? {
            return Err(status_error(response.StatusCode()?.0));
        }

        let content = response.Content()?;
        // `ContentLength()` itself — not just `.Value()` — throws when the
        // response declares no Content-Length at all, which is the common case
        // for a chunked/gzip response (GitHub's release API, iTunes Search).
        // Treating that the same as "no declared length" rather than
        // propagating it was the bug: every such response made the whole
        // request fail with a bogus S_OK-coded error, which silently broke
        // both the update check and the artwork lookup.
        if let Ok(declared) = declared_content_length(&content) {
            if declared > MAX_RESPONSE_BYTES {
                return Err(too_large());
            }
        }

        let stream = join_with_timeout(&content.ReadAsInputStreamAsync()?, remaining(deadline))?;
        let bytes = read_capped(&stream, deadline)?;
        String::from_utf8(bytes).map_err(|_| {
            WinError::new(
                HRESULT::from_win32(ERROR_INVALID_DATA.0),
                "the response was not valid UTF-8",
            )
        })
    }
}

/// The `Content-Length` declared on a response body, if any.
///
/// `HttpContentHeaderCollection::ContentLength()` throws — not just its
/// `.Value()` — when the header was not sent at all, rather than yielding a
/// null `IReference<u64>`. Bundling both fallible steps behind one
/// `WinResult` lets the caller treat "the header is missing" and "the header
/// is present but null" the same way: as "no declared length", not as a
/// reason to fail the whole request.
fn declared_content_length(content: &IHttpContent) -> WinResult<u64> {
    content.Headers()?.ContentLength()?.Value()
}

/// Reads a stream to its end, giving up past `MAX_RESPONSE_BYTES`.
///
/// A `Content-Length` covers the ordinary case, but a chunked response declares
/// no length at all, so the cap has to hold on the read path too.
fn read_capped(stream: &IInputStream, deadline: Instant) -> WinResult<Vec<u8>> {
    const CHUNK: u32 = 16 * 1024;

    let mut out: Vec<u8> = Vec::new();
    loop {
        let buffer = Buffer::Create(CHUNK)?;
        let filled = join_with_timeout(
            &stream.ReadAsync(&buffer, CHUNK, InputStreamOptions::Partial)?,
            remaining(deadline),
        )?;
        let filled_len = filled.Length()?;
        if filled_len == 0 {
            return Ok(out);
        }
        if out.len() as u64 + u64::from(filled_len) > MAX_RESPONSE_BYTES {
            return Err(too_large());
        }
        let start = out.len();
        out.resize(start + filled_len as usize, 0);
        DataReader::FromBuffer(&filled)?.ReadBytes(&mut out[start..])?;
    }
}

fn too_large() -> WinError {
    WinError::new(
        HRESULT::from_win32(ERROR_FILE_TOO_LARGE.0),
        "the response was larger than this app will read",
    )
}

/// A refused request, carrying the status both ways it can be read.
///
/// `0x8019_0000 | status` is the same `HTTP_E_STATUS_*` encoding WinRT itself
/// uses, so anything decoding the HRESULT still recognises it; the message is
/// there because the HRESULT is what ends up in a log line, and `0x80190193`
/// is not a thing anyone should have to convert in their head.
fn status_error(status: i32) -> WinError {
    WinError::new(
        HRESULT(0x8019_0000u32 as i32 | status),
        format!("the server answered HTTP {status}"),
    )
}

fn remaining(deadline: Instant) -> Duration {
    deadline.saturating_duration_since(Instant::now())
}
