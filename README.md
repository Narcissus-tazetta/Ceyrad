# Ceyrad

[日本語版はこちら](README.ja.md)

A macOS menu bar app that shows the track currently playing in Apple Music or Spotify as your Discord status (Rich Presence).

- Shows title, artist, album art, and a playback progress bar as "Listening to ~"
- Up to 2 buttons (song / artist / album page, custom URL, repository)
- Supports Apple Music and Spotify; if both are open, whichever is actually playing is shown
- **Does nothing when neither player is running** (no polling — fully event-driven)

## Requirements

- macOS 13 or later
- Discord desktop app

## Installation

1. Download the latest `Ceyrad.app` from [Releases](https://github.com/Narcissus-tazetta/Ceyrad/releases) and move it to `/Applications`
2. Open `Ceyrad.app`
   - Since this is an independently distributed app (ad-hoc signed, not notarized by Apple), macOS may show "cannot verify developer" on first launch. If so, use either of the following to open it:
     - Right-click the app in Finder and choose "Open"
     - If it's still blocked, open "System Settings > Privacy & Security", scroll down to the message saying `"Ceyrad" was blocked to protect your Mac`, and click "Open Anyway"
3. If a ♪ icon appears in the menu bar, it launched successfully

## Usage

1. Play a track in Apple Music or Spotify
2. On first use, macOS will ask "Allow Ceyrad to control Apple Music?" (and separately for Spotify) — **Allow** it
   (used to get the playback position and artwork not included in notifications; if denied, the app still works with reduced info)
3. If Discord is already running, your status updates within a few seconds. If you launch Discord afterward, it's detected automatically and connects

## Menu bar settings

| Item | Description |
|---|---|
| Button 1 / Button 2 | Link target: song page / artist page / album page / custom URL / repository / disabled. "Change Label…" also lets you edit the button's display text (up to 32 characters) |
| Set Custom URL… | The URL used when a button's link target is "custom URL" |
| Set Repository URL… | The repository button's target URL |
| Music Sources | Which players to watch: Apple Music / Spotify (both enabled by default). If both are playing, the one with the most recent activity is shown; a paused player yields to a playing one |
| Status Badge | What the compact "Listening to …" badge (member list, DM sidebar, etc.) shows: app name / artist name / track name (default: artist name) |
| When Paused | Behavior on pause: keep showing / clear immediately / clear after 1, 3, 5, or 10 minutes (default: 5 minutes) |
| Launch at Login | Automatically start Ceyrad at login (click to toggle on/off) |
| Language | Switches the display language of the menu (this settings UI) between English / 日本語. Text shown on the Discord side (connection status, button labels, etc.) is not affected and always stays in English |
| Reconnect to Discord | Retries the connection to Discord |
| Check for Updates… | Check for and install the latest version via [Sparkle](#auto-update-sparkle) |

## Auto-launch at login (optional)

Click "Launch at Login" in the menu bar to turn it on.
(Manually adding `Ceyrad.app` to "System Settings > General > Login Items" has the same effect.)

## FAQ

**The current track doesn't show up when I (re)launch the app while music is playing**
Getting the current track info at launch requires Automation permission. Check "System Settings > Privacy & Security > Automation" to make sure "Music" is enabled. If you can't enable it or it's not in the list, restart the app once to bring back the permission dialog. Even without this permission, notifications for track changes, pauses, etc. still arrive, so it will update from the next action onward (the playback progress bar just won't appear). If the menu bar shows "Music Control: Not Authorized ⚠️", this is the cause.

**The buttons don't show up on my own profile**
This is a Discord limitation — RPC buttons aren't visible to yourself. Have another account or another person check.

**Some tracks don't show album art**
Artwork and links are resolved via the iTunes Search API (the Apple Music catalog). Locally imported tracks or tracks not in the catalog will have no artwork, and catalog-based buttons (song/artist/album) are hidden. Variations in notation like "feat." or "- Single" are normalized, but if your macOS "Language & Region" region differs from your Apple Music storefront (country), the track may not exist in that country's catalog and fail to resolve.

**I launched Discord afterward**
It's detected automatically and connects (if a player is running, the status appears within a few seconds). If it doesn't, use "Reconnect to Discord" from the menu.

**My Spotify status shows up twice**
If you have linked Spotify in Discord's own settings (Connections > Spotify with "Display Spotify as your status" on), Discord shows its own presence in addition to Ceyrad's. Turn one of them off — either Discord's built-in display or Ceyrad's Spotify source in "Music Sources".

**Spotify ads / local files look odd**
Spotify ads and locally imported files are shown as regular tracks but without artwork, and catalog-based buttons (song/artist/album) are hidden.

---

## Development

The rest of this document is for people building from source or contributing.

See [DESIGN.md](DESIGN.md) for design details.

### Build

Requires Xcode Command Line Tools (Swift 5.9+).

```sh
./make-app.sh
mv Ceyrad.app /Applications/
```

Building as a `.app` ties the Automation permission to the app itself (avoiding permission issues tied to the launching terminal), and lets it be registered in "Login Items" in System Settings.

If you just want a plain executable, use `swift build -c release` (`.build/release/Ceyrad`). To auto-launch at login via a LaunchAgent, create a plist like the following at `~/Library/LaunchAgents/local.ceyrad.plist`.

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>local.ceyrad</string>
	<key>ProgramArguments</key>
	<array>
		<string>/path/to/.build/release/Ceyrad</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<false/>
</dict>
</plist>
```

```sh
launchctl load ~/Library/LaunchAgents/local.ceyrad.plist
```

### Discord Application ID

The Application IDs used to connect to Discord are hardcoded in [Sources/Ceyrad/SettingsStore.swift](Sources/Ceyrad/SettingsStore.swift) and are dedicated to this app. The Application *name* is what Discord shows as "Listening to ~", so there is one Application per source: `SettingsStore.discordClientId` (named **Apple Music**) and `SettingsStore.spotifyDiscordClientId` (named **Spotify**; Ceyrad reconnects with this ID when the active source switches). If you fork this for your own use, create your own Applications in the [Discord Developer Portal](https://discord.com/developers/applications) with those names and replace the IDs.

### Tests

```sh
swift test
```

`Tests/CeyradTests` covers pure logic such as ActivityBuilder payload generation, iTunes search matching, and AppleScript output parsing.

### Lint

```sh
./scripts/lint.sh          # swift-format lint + swiftlint
./scripts/lint.sh --fix    # swift-format format -i + swiftlint --fix
```

Configured with [SwiftLint](https://github.com/realm/SwiftLint) (`.swiftlint.yml`) and [swift-format](https://github.com/swiftlang/swift-format) (`.swift-format`, using the `swift format` command bundled with the Swift toolchain). [`ci.yml`](.github/workflows/ci.yml) runs lint, tests, and build on every PR and push.

### Auto-update (Sparkle)

[Sparkle](https://sparkle-project.org/) lets you check for updates from within the app ("Check for Updates…" in the menu, or a once-a-day background check via `SUEnableAutomaticChecks`).

- Update info is fetched from [`appcast.xml`](appcast.xml) (referenced on the `main` branch via raw.githubusercontent.com)
- The distributed artifact itself is verified with an EdDSA signature (since code signing is ad-hoc, this is the actual tamper-detection mechanism)

#### For maintainers: setting up the signing key (one-time)

To have `appcast.xml` automatically updated on every new release, you need to register the EdDSA signing key as a GitHub Actions secret.

1. Run `generate_keys` from a Sparkle release ([Sparkle-*.tar.xz](https://github.com/sparkle-project/Sparkle/releases), or `.build/artifacts/sparkle/Sparkle/bin/` after `swift package resolve`)

   ```sh
   ./generate_keys
   ```

   The key is stored in the macOS Keychain, and the public key is printed. **This public key is already set in `Support/Info.plist`'s `SUPublicEDKey`** (as of when this setup was done). If you regenerate the key, update it there too.

2. Export the private key (never commit the contents of this file)

   ```sh
   ./generate_keys -x /tmp/sparkle_private_key.txt
   ```

3. In the GitHub repository, go to **Settings > Secrets and variables > Actions** and create a secret named `SPARKLE_PRIVATE_KEY`, pasting in the contents of the file above. Once registered, delete `/tmp/sparkle_private_key.txt`.

If the secret isn't set, the release workflow will just publish the zip/dmg as usual, skipping the `appcast.xml` update (the app still runs, but auto-update won't work).

### Release

Pushing a tag in the form `v*.*.*` triggers GitHub Actions (`.github/workflows/release.yml`) to build automatically, zip `Ceyrad.app`, and attach it to a GitHub Release.

```sh
git tag v1.0.0
git push origin v1.0.0
```

Since the distributed zip is ad-hoc signed (no Apple Developer certificate signing or notarization), Gatekeeper will show "cannot verify developer" on first launch after downloading. In that case, right-click the app in Finder and choose "Open", or remove the quarantine attribute with `xattr -cr Ceyrad.app`.

When you push a tag, `Support/Info.plist`'s `CFBundleShortVersionString`/`CFBundleVersion` are automatically rewritten to the tag's version (e.g. `v1.2.0` → `1.2.0`) before building. If `SUPublicEDKey` is set and the `SPARKLE_PRIVATE_KEY` secret is registered, signing the update zip and appending/pushing to [appcast.xml](#auto-update-sparkle) also happens in the same workflow.
