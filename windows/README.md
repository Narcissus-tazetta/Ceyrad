# Ceyrad for Windows (work in progress)

A Rust rewrite of [Ceyrad](../README.md) targeting Windows. **Not usable yet** —
there is no tray app to run. What exists today is the ported logic plus two
probes for verifying the risky platform assumptions on real hardware.

## Status

| Piece | State |
|---|---|
| `src/core/` — activity payload, source selection, settings, debounce, status lines | Ported from Swift, 62 tests passing |
| `src/discord/protocol.rs` — IPC framing | Done (pure); the named-pipe transport is not |
| `src/bin/smtc_probe.rs` | Ready to run — needs verification on a real machine |
| `src/bin/discord_probe.rs` | Ready to run — needs verification against a live Discord |
| Tray icon / menu, SMTC watcher, orchestrator, settings file, autostart, updater | Not started |

## Running the tests

The `core` and `discord::protocol` modules have no platform calls, so this works
on any OS:

```bash
cargo test
```

## Probe 1: what does SMTC report?

Prints every media session's `SourceAppUserModelId` (AUMID), metadata, playback
status and timeline, then watches for change events for two minutes.

This answers the question that blocks everything downstream: **which AUMID
strings identify Spotify and Apple Music on Windows**, and whether an exact
match is safe or a prefix match is needed.

```bash
cargo run --bin smtc_probe
```

Start playback in Spotify and/or Apple Music first, then play, pause and skip
while it watches. Worth capturing:

- the exact `AUMID:` line for each player (copy verbatim)
- whether `position` in seconds matches the elapsed time the player shows
  on screen — this catches a unit mistake before anything depends on it
- whether the events fire on play/pause/skip, and whether `SessionsChanged`
  fires when a player is closed and reopened

## Probe 2: does Discord IPC work over a named pipe?

Connects to `\\.\pipe\discord-ipc-0..9`, handshakes, waits for `READY`, pushes a
test activity, then clears it after 60 seconds.

```bash
cargo run --bin discord_probe
# or, to test the Spotify application id:
cargo run --bin discord_probe spotify
```

Discord must be running. Buttons and the "Listening to" status are not visible
on your own profile — check from another account. Worth capturing: whether
`READY` arrives, whether the status appears, and what happens if Discord is
quit while the probe is connected.
