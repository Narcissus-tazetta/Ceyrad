# Ceyrad for Windows

A Rust rewrite of [Ceyrad](../README.md) targeting Windows. It sits in the
notification area, the way the macOS build sits in the menu bar, and everything
the macOS menu can do is in its menu too.

## Status

| Piece | State |
|---|---|
| `src/core/` — activity payload, source selection, settings, debounce, status lines, AUMID matching, iTunes matching, change detection, menu rows, tray glyph, update comparison | Ported from Swift |
| `src/discord/` — IPC framing, named-pipe transport, connection state machine | Done |
| `src/smtc/` — session watcher | Done |
| `src/catalog/` — iTunes Search lookup, on a thread of its own | Done |
| `src/winhttp/` — blocking HTTP with a deadline, shared by the lookups | Done |
| `src/app/` — orchestrator, settings file | Done |
| `src/tray/` — notification-area icon, menu, dialogs | Done |
| `src/launch_at_login.rs` — start with Windows | Done |
| `src/updater/` — notices a newer release | Done (check only, no install) |
| Spotify song/artist/album buttons | Not planned — see [Artwork and links](#artwork-and-links) |
| `installer/` — Inno Setup installer | Done — see [Installing](#installing) |
| Code signing | Not planned — matches the macOS build, which also ships unsigned |

191 tests cover everything above that is not a platform call.

## Running it

```bash
cargo run --release --bin ceyrad
```

There are two binaries, the same orchestrator either way:

- **`ceyrad`** — the real one. No console, no taskbar button; an icon appears in
  the notification area (Windows files new icons under the `^` overflow until
  you drag them out). Right- or left-click it for the menu.
- **`ceyrad_dev`** — a console build that prints every decision it makes.
  Nothing to look at, but it is the only place to watch the reasoning, so it is
  what to reach for when something misbehaves. Ctrl+C clears the presence and
  exits.

Settings live in `%APPDATA%\Ceyrad\settings.json`, written by the menu as you
change things. The file is plain JSON and can be edited by hand instead; missing
keys fall back to defaults, so a partial file is fine. Spotify is off by default,
matching the macOS build — Discord ships its own Spotify integration.

## Installing

Every release ships two ways — pick either, they don't conflict:

- **`Ceyrad-vX.Y.Z-windows-setup.exe`** — an installer built with
  [Inno Setup](https://jrsoftware.org/isinfo.php) (`installer/ceyrad.iss`). It
  installs to `%LOCALAPPDATA%\Programs\Ceyrad`, adds a Start Menu entry (and,
  if ticked, a desktop shortcut), registers an uninstaller in "Add or Remove
  Programs", and offers to launch the app when it finishes. No elevation
  prompt: the install location, like everything else this app touches, needs
  nothing beyond the current user's own permissions.
- **`Ceyrad-vX.Y.Z-windows.zip`** — the portable build. Unpack `ceyrad.exe`
  anywhere and run it; nothing is written outside `%APPDATA%\Ceyrad`.

There is no code signing certificate — matching the macOS build, which also
ships unsigned — so SmartScreen will warn about an unrecognised app the first
time either way. That is what an unsigned binary looks like, and there is no
way around it short of a certificate.

Running the installer while an old copy is already sitting in the tray is the
normal upgrade path, not a special case: Setup uses Restart Manager to find
and terminate the running `ceyrad.exe` before it overwrites the file. (It has
to force the close — the tray's message loop doesn't respond to Restart
Manager's polite shutdown request — which is safe here because nothing is
lost: every setting is written to `settings.json` the moment it changes, not
on exit.) Uninstalling does the same before removing the install directory, so
it doesn't leave a running process pointing at a deleted exe.

Uninstalling removes the install directory and Start Menu entry only.
`%APPDATA%\Ceyrad\settings.json` and any "Launch at Login" registration are
left alone, since those are user preferences, not install artifacts — see
below.

**Launch at Login** in the menu writes a value under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, which is per-user and needs
no elevation. It is the same entry Task Manager's Startup tab lists, so it can be
switched off from there too and the menu will agree. The installer does not
touch this key itself — the menu's toggle is the only thing that ever writes
it — but moving an installed copy (or removing it without disabling the toggle
first) would otherwise leave that entry pointing at nothing, so it is
rewritten to the current location every time the app starts.

### Building the installer locally

```bash
cargo build --release --bin ceyrad
iscc installer\ceyrad.iss /DMyAppVersion=1.2.3
```

Needs [Inno Setup 6](https://jrsoftware.org/isdl.php) (`iscc` on `PATH`, or use
its full path — the copy under `%LOCALAPPDATA%\Programs\Inno Setup 6` if it was
installed per-user rather than machine-wide). Without `/DMyAppVersion`, the
script falls back to a placeholder version, which is fine for a local test
build but not what CI passes. Output lands in `installer\dist\`.

## Updates

The menu's **Check for Updates…** asks GitHub for the newest release and compares
it against this build. When one is newer, the row becomes **Update Available:
vX.Y.Z** and clicking it opens the release page in your browser. It also checks
once shortly after launch and then daily, matching the macOS build's schedule.

It stops at telling you. macOS uses Sparkle to download and swap the app in
place, which works because that build is signed; there is no certificate here, so
an automatic replace would be handing you an unsigned binary with nothing to
verify it against. Downloading it yourself, from a page you can look at first, is
the honest version of that until signing exists.

### Why the tray does not cost anything

The icon's window, Discord's pipe, the media-session watcher and the app's own
timers are all waited on together, in one thread, by a single
`MsgWaitForMultipleObjects`. Nothing polls, so an idle app measures 0 ms of CPU
between songs — adding a UI did not change that.

Opening the menu runs a nested message loop for as long as it is on screen, so
player and Discord events are noticed late rather than promptly while it is
open. Nothing is dropped: those events are kernel handles and flags that stay
set until read. macOS behaves the same way while a menu is down.

### If your player is not detected

The AUMID a player reports depends on how it was installed, so an install we
have not seen is simply ignored — and logged:

```
[14:02:11] smtc: ignoring unrecognised session SomePublisher.SomeApp_abc!App
```

Copy that id into an environment variable and it will be treated as that source
(comma-separated for more than one, and a package-family prefix works too):

```bash
CEYRAD_AUMID_APPLE_MUSIC="AppleInc.AppleMusicWin" cargo run --bin ceyrad_dev
```

If you find the real id for a stock install, it belongs in
`src/core/aumid.rs` rather than in an environment variable.

### Artwork and links

SMTC reports a title, an artist and a position, but no URL — and its thumbnail
is a byte stream, where Discord's `large_image` wants something it can fetch. So
artwork and the song/artist/album buttons come from the iTunes Search API, the
same source the macOS build uses, on a thread of its own (`src/catalog/`). The
first presence for a track goes out without them and is re-sent once the lookup
lands, so a slow network delays the artwork rather than the card.

The storefront follows the machine's region, so a Japanese install gets
`music.apple.com/jp` links. Tracks that are not in the catalog — local imports,
mostly — resolve to nothing and keep just the repository button. A lookup that
fails outright (offline, or the API refusing) is retried after 15 seconds
rather than costing the track its artwork for good; a lookup that simply finds
nothing is not retried, because that is a real answer.

**Spotify gets the artwork but no links, and that is by design, not a gap to
close.** Every URL this API returns points at Apple Music, and under a Spotify
presence the button reads "Play on Spotify" — sending it to `music.apple.com`
would be worse than having no button. The cover art is the same record either
way, so that much crosses over and the song, artist and album buttons simply do
not appear. Real Spotify links would need a Spotify-side lookup (Web API search
by title/artist, since SMTC never reports a track id) ported from scratch —
skipped on purpose, since Discord already ships its own Spotify integration and
Spotify monitoring is off by default here anyway.

### Apple Music's metadata shape

Apple Music for Windows leaves SMTC's `AlbumTitle` empty and packs the album
into the artist field instead, as `Mrs. GREEN APPLE — Brand New - Single`.
`src/core/track_metadata.rs` splits it back apart; without that the artist row
carries the album and the catalog search matches nothing.

## Running the tests

```bash
cargo test
```

The `core` and `discord::protocol` modules have no platform calls, so most of
the suite runs on any OS. The settings-file tests live under `src/app/`, which
is Windows-only, and are skipped elsewhere along with the rest of that module.

## Probe 1: what does SMTC report?

Prints every media session's `SourceAppUserModelId` (AUMID), metadata, playback
status and timeline, then watches for change events for two minutes.

This is how to find the AUMID for an install the dev build does not recognise.

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
