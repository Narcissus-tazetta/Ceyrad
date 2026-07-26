//! Named-pipe transport for Discord IPC, using overlapped (async) I/O.
//!
//! Discord listens on `\\.\pipe\discord-ipc-0..9`. The obvious implementation —
//! open the pipe synchronously and call `ReadFile` — cannot be given a deadline:
//! an idle connection carries no traffic at all, so the call parks in the kernel
//! until Discord happens to send something, which may be never. Anything the
//! caller wanted to do on a timer (clear a stale presence, fire a debounced
//! push, notice a settings change) never runs.
//!
//! Opening with `FILE_FLAG_OVERLAPPED` fixes that: the read is issued, the
//! caller waits on the read's event *and* whatever else it cares about, and an
//! unfinished read is cancelled before returning. Cancellation still reports
//! bytes that landed in the race window, so nothing is dropped.

use std::io;
use std::time::Duration;

use serde_json::Value;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{
    CloseHandle, ERROR_BROKEN_PIPE, ERROR_INVALID_HANDLE, ERROR_IO_PENDING, ERROR_NO_DATA,
    ERROR_OPERATION_ABORTED, ERROR_PIPE_NOT_CONNECTED, HANDLE, WAIT_EVENT, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE,
    OPEN_EXISTING,
};
use windows::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForSingleObject, INFINITE,
};
use windows::Win32::System::IO::{CancelIo, GetOverlappedResult, OVERLAPPED};
use windows::Win32::UI::WindowsAndMessaging::{MsgWaitForMultipleObjects, QS_ALLINPUT};

use super::protocol::{encode_frame, Opcode};

const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

/// A write that cannot finish in this long means Discord has stopped draining
/// the pipe; treated as a dead connection rather than blocking the event loop.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Why a wait returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wakeup {
    /// Bytes were read into the caller's buffer.
    Data(usize),
    /// The extra handle passed by the caller was signalled.
    Signaled,
    /// A window message is queued for this thread. The caller must drain the
    /// queue before waiting again — every wait here includes `QS_ALLINPUT`, so
    /// a message left unread would wake the next wait immediately and spin.
    Message,
    /// The deadline passed with nothing to report.
    Timeout,
    /// Discord closed the pipe.
    Closed,
}

/// Manual-reset event, used both internally for overlapped completion and by
/// callers as a cross-thread wakeup.
#[derive(Debug)]
pub struct Event(HANDLE);

// The handle is just a kernel object reference; the Win32 event API is
// explicitly safe to call from any thread.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

impl Event {
    pub fn new() -> io::Result<Self> {
        let handle =
            unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(io::Error::other)?;
        Ok(Self(handle))
    }

    pub fn handle(&self) -> HANDLE {
        self.0
    }

    pub fn set(&self) -> io::Result<()> {
        unsafe { SetEvent(self.0) }.map_err(io::Error::other)
    }

    pub fn reset(&self) -> io::Result<()> {
        unsafe { ResetEvent(self.0) }.map_err(io::Error::other)
    }

    /// Blocks until signalled, a window message arrives, or `timeout` elapses.
    /// `None` waits forever.
    pub fn wait(&self, timeout: Option<Duration>) -> Wakeup {
        let handles = [self.0];
        match wait_with_messages(&handles, timeout) {
            WAIT_OBJECT_0 => Wakeup::Signaled,
            wait if is_message_wakeup(wait, handles.len()) => Wakeup::Message,
            _ => Wakeup::Timeout,
        }
    }
}

#[derive(Debug)]
pub struct Pipe {
    handle: HANDLE,
    read_event: Event,
    write_event: Event,
}

impl Drop for Pipe {
    fn drop(&mut self) {
        // Any read still pending belongs to this handle; cancel it so the
        // kernel isn't left writing into a buffer we are about to forget.
        unsafe {
            let _ = CancelIo(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}

impl Pipe {
    /// Tries `discord-ipc-0` through `-9` and keeps the first that opens.
    pub fn connect() -> io::Result<Self> {
        let mut last_err = None;
        for i in 0..10 {
            let path = format!(r"\\.\pipe\discord-ipc-{i}");
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide.as_ptr()),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_MODE(0),
                    None,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                    None,
                )
            };
            match handle {
                Ok(handle) if !handle.is_invalid() => {
                    return Ok(Self {
                        handle,
                        read_event: Event::new()?,
                        write_event: Event::new()?,
                    });
                }
                Ok(_) => {}
                Err(e) => last_err = Some(e),
            }
        }
        Err(io::Error::other(format!(
            "no discord-ipc pipe found (is Discord running?); last error: {last_err:?}"
        )))
    }

    pub fn send(&self, opcode: Opcode, payload: &Value) -> io::Result<()> {
        self.write_all(&encode_frame(opcode, payload))
    }

    /// Issues an overlapped read, then waits until it completes, `extra` is
    /// signalled, or `timeout` elapses (`None` waits forever).
    ///
    /// A read that has not completed is cancelled before returning. Bytes that
    /// arrived while the cancel was in flight are still reported as `Data`, so
    /// waking up for another reason never costs a frame.
    pub fn read_or_signal(
        &self,
        buf: &mut [u8],
        extra: Option<HANDLE>,
        timeout: Option<Duration>,
    ) -> io::Result<Wakeup> {
        self.read_event.reset()?;
        let mut overlapped = OVERLAPPED {
            hEvent: self.read_event.handle(),
            ..Default::default()
        };

        match unsafe { ReadFile(self.handle, Some(buf), None, Some(&mut overlapped)) } {
            // Completed inline; the event is already signalled.
            Ok(()) => {}
            Err(e) if is_code(&e, ERROR_IO_PENDING.0) => {}
            Err(e) if is_disconnect(&e) => return Ok(Wakeup::Closed),
            Err(e) => return Err(io::Error::other(e)),
        }

        let mut handles = [self.read_event.handle(); 2];
        let count = match extra {
            Some(extra) => {
                handles[1] = extra;
                2
            }
            None => 1,
        };
        let wait = wait_with_messages(&handles[..count], timeout);

        if wait == WAIT_OBJECT_0 {
            return Ok(match self.finish_read(&overlapped, false)? {
                // An aborted read cannot happen on this path, but treating it
                // as a spurious wakeup is harmless and keeps the loop alive.
                ReadResult::Aborted => Wakeup::Timeout,
                ReadResult::Data(n) => Wakeup::Data(n),
                ReadResult::Closed => Wakeup::Closed,
            });
        }

        // Woken for another reason, or out of time: take the read back.
        unsafe {
            let _ = CancelIo(self.handle);
        }
        match self.finish_read(&overlapped, true)? {
            ReadResult::Data(n) => Ok(Wakeup::Data(n)),
            ReadResult::Closed => Ok(Wakeup::Closed),
            ReadResult::Aborted => Ok(match wait {
                WAIT_TIMEOUT => Wakeup::Timeout,
                wait if is_message_wakeup(wait, count) => Wakeup::Message,
                _ => Wakeup::Signaled,
            }),
        }
    }

    /// Collects the outcome of an overlapped read. `wait` blocks until the
    /// kernel is done with the OVERLAPPED, which is what makes it safe to let a
    /// stack-allocated one go out of scope afterwards.
    fn finish_read(&self, overlapped: &OVERLAPPED, wait: bool) -> io::Result<ReadResult> {
        let mut transferred = 0u32;
        match unsafe { GetOverlappedResult(self.handle, overlapped, &mut transferred, wait) } {
            // A successful zero-byte read on a pipe means the peer hung up.
            Ok(()) if transferred == 0 => Ok(ReadResult::Closed),
            Ok(()) => Ok(ReadResult::Data(transferred as usize)),
            Err(e) if is_code(&e, ERROR_OPERATION_ABORTED.0) => Ok(ReadResult::Aborted),
            Err(e) if is_disconnect(&e) => Ok(ReadResult::Closed),
            Err(e) => Err(io::Error::other(e)),
        }
    }

    fn write_all(&self, mut buf: &[u8]) -> io::Result<()> {
        while !buf.is_empty() {
            self.write_event.reset()?;
            let mut overlapped = OVERLAPPED {
                hEvent: self.write_event.handle(),
                ..Default::default()
            };

            match unsafe { WriteFile(self.handle, Some(buf), None, Some(&mut overlapped)) } {
                Ok(()) => {}
                Err(e) if is_code(&e, ERROR_IO_PENDING.0) => {}
                Err(e) if is_disconnect(&e) => {
                    return Err(io::Error::other("pipe closed during write"))
                }
                Err(e) => return Err(io::Error::other(e)),
            }

            let waited = unsafe {
                WaitForSingleObject(self.write_event.handle(), timeout_ms(Some(WRITE_TIMEOUT)))
            };
            if waited != WAIT_OBJECT_0 {
                unsafe {
                    let _ = CancelIo(self.handle);
                    let mut ignored = 0u32;
                    let _ = GetOverlappedResult(self.handle, &overlapped, &mut ignored, true);
                }
                return Err(io::Error::other("timed out writing to the Discord pipe"));
            }

            let mut written = 0u32;
            unsafe { GetOverlappedResult(self.handle, &overlapped, &mut written, true) }.map_err(
                |e| {
                    if is_disconnect(&e) {
                        io::Error::other("pipe closed during write")
                    } else {
                        io::Error::other(e)
                    }
                },
            )?;
            if written == 0 {
                return Err(io::Error::other("pipe closed during write"));
            }
            buf = &buf[written as usize..];
        }
        Ok(())
    }
}

enum ReadResult {
    Data(usize),
    Closed,
    Aborted,
}

/// Waits on `handles`, but also returns as soon as a window message is queued
/// for this thread.
///
/// Plain `WaitForMultipleObjects` would leave a tray click sitting in the queue
/// until Discord or a player happened to say something — which, on an idle
/// connection between songs, can be never. Including `QS_ALLINPUT` is what lets
/// one loop serve the pipe, the music watcher, the app's timers and the menu.
/// A thread that never made a window simply never sees this fire.
fn wait_with_messages(handles: &[HANDLE], timeout: Option<Duration>) -> WAIT_EVENT {
    unsafe { MsgWaitForMultipleObjects(Some(handles), false, timeout_ms(timeout), QS_ALLINPUT) }
}

/// `MsgWaitForMultipleObjects` reports a message as the slot one past the last
/// handle it was given.
fn is_message_wakeup(wait: WAIT_EVENT, handle_count: usize) -> bool {
    wait.0 == WAIT_OBJECT_0.0 + handle_count as u32
}

fn timeout_ms(timeout: Option<Duration>) -> u32 {
    match timeout {
        None => INFINITE,
        // INFINITE is 0xFFFFFFFF, so a very long finite wait must not round up
        // into it; clamping one below keeps "long" from becoming "forever".
        Some(d) => u32::try_from(d.as_millis())
            .unwrap_or(INFINITE - 1)
            .min(INFINITE - 1),
    }
}

fn is_code(error: &windows::core::Error, code: u32) -> bool {
    error.code() == windows::core::HRESULT::from_win32(code)
}

fn is_disconnect(error: &windows::core::Error) -> bool {
    [
        ERROR_BROKEN_PIPE.0,
        ERROR_PIPE_NOT_CONNECTED.0,
        ERROR_NO_DATA.0,
        ERROR_INVALID_HANDLE.0,
    ]
    .iter()
    .any(|code| is_code(error, *code))
}
