# Forvid

A lightweight native desktop app for downloading videos from the web —
powered by [yt-dlp](https://github.com/yt-dlp/yt-dlp), wrapped in a clean UI.

Forvid gives you the power of yt-dlp without the command line: paste a link,
pick a quality, and watch it download. No browser extensions, no ads, no
sketchy downloader websites. **No setup either** — on first launch, Forvid
downloads its own copy of yt-dlp, ffmpeg and deno into its private folder
and manages them for you. (If you already have them installed, it simply
uses those.)

---

## Get Forvid (the easy way)

No technical knowledge needed — download the app, open it, done. Forvid
installs everything it needs by itself the first time you open it.

### On a Mac

1. Open the [Releases page](https://github.com/geraldofori/forvid/releases)
   and download the file ending in **`.dmg`** (pick **aarch64** for any Mac
   with an M1/M2/M3/M4 chip, **x64** for older Intel Macs).
2. Double-click the downloaded file. A window opens — **drag Forvid onto
   the Applications folder**.
3. Open Forvid from your Applications folder. The very first time, your Mac
   may warn that the app is from an unidentified developer:
   **right-click the app → Open → Open**. You only have to do this once.
4. Forvid downloads its tools by itself (about 120 MB, one time only) —
   then paste a video link and you're off.

### On Windows

1. Open the [Releases page](https://github.com/geraldofori/forvid/releases)
   and download the file ending in **`-setup.exe`**.
2. Double-click it to install. Windows may show a blue "Windows protected
   your PC" message — click **More info → Run anyway**. (The app simply
   isn't signed with a paid certificate; it's safe.)
3. Open Forvid from the Start menu. On first launch it downloads its tools
   by itself (about 120 MB, one time only).
4. Paste a video link and download.

### On Linux

1. Open the [Releases page](https://github.com/geraldofori/forvid/releases)
   and download the file ending in **`.AppImage`**.
2. Make it runnable: right-click the file → **Properties → Permissions** →
   tick **"Allow executing file as program"**.
3. Double-click it to open Forvid.
4. One small extra step on Linux: install **ffmpeg** from your app store or
   package manager (on Ubuntu: `sudo apt install ffmpeg`). Forvid takes
   care of everything else itself.

---

## What it does

- **Paste a link, get the video.** Forvid fetches the video's title, channel,
  duration and thumbnail before you download anything, so you always know
  what you're getting.
- **Whole playlists, one click.** Paste a playlist link and Forvid offers to
  download the entire thing into a new folder named after the playlist,
  with "Video 3 of 24" progress along the way.
- **Clipboard aware.** Copy a link anywhere on your Mac, switch back to
  Forvid, and it offers to use it — one click, no pasting.
- **Real quality choice.** The quality dropdown is built from the resolutions
  the video *actually* has (no offering 4K for a 720p upload), plus
  **Audio only (MP3)** for music and podcasts. Defaults to a sensible medium
  (~720p) instead of the biggest file.
- **Honest progress.** Percent, download speed, ETA, and bytes downloaded vs
  total — streamed live from yt-dlp. Cancel any download mid-flight.
- **You choose where files go.** Downloads land in `~/Downloads` by default;
  change the destination with the native macOS folder picker.
- **Works with your system.** Dark and light mode (follows your Mac by
  default, or pick one), native notification when a download completes, and
  **Reveal in Finder** to jump straight to the file.

## Capabilities at a glance

| | |
|---|---|
| Sites supported | Everything yt-dlp supports (YouTube and [thousands more](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md)) |
| Video quality | Best available, or capped at any resolution the video offers (up to 4K+) |
| Audio | MP3 extraction via ffmpeg |
| Container | Merges high-quality video+audio into MP4 |
| Playlists | Full playlists, saved into a folder named after the playlist |
| Appearance | Dark and light mode — follows the system, or pick manually |
| Concurrency | One download at a time (a queue is planned) |
| Size | ~12 MB installed (Tauri, not Electron) |

## Build from source

Only needed if you want to compile Forvid yourself (for example to develop
on it) — **not** needed to use the app; see
[Get Forvid (the easy way)](#get-forvid-the-easy-way) above. You'll need
[Node.js](https://nodejs.org) and [Rust](https://www.rust-lang.org/tools/install)
(`rustup`).

### macOS

```sh
# 1. Rust, if you don't have it
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh

# 2. Build and install
git clone https://github.com/geraldofori/forvid.git
cd forvid
npm install
npm run tauri build
cp -R src-tauri/target/release/bundle/macos/Forvid.app /Applications
```

The DMG is at `src-tauri/target/release/bundle/dmg/`.

Because Forvid is built ad-hoc signed (no Apple Developer account), the first
launch may be blocked by Gatekeeper. If that happens: **right-click the app →
Open → Open**.

### Windows

1. **WebView2** — already present on Windows 10 (1803+) and 11. Otherwise get
   the [Evergreen Bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/).
2. **Microsoft C++ Build Tools** — from the
   [Visual Studio installer](https://visualstudio.microsoft.com/visual-cpp-build-tools/),
   select "Desktop development with C++".
3. **Rust** — `winget install Rustlang.Rustup` (choose the MSVC toolchain).
4. **Build**:

   ```powershell
   git clone https://github.com/geraldofori/forvid.git
   cd forvid
   npm install
   npm run tauri build
   ```

   The app and installers land in `src-tauri\target\release\bundle\`
   (`nsis` / `msi` installers, or the standalone `app.exe` in
   `src-tauri\target\release\`).

### Linux

1. **Tauri's system dependencies** (Debian/Ubuntu — see the
   [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for other
   distros):

   ```sh
   sudo apt update
   sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
     libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
   ```

2. **Rust** — `curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh`
3. **Build**:

   ```sh
   git clone https://github.com/geraldofori/forvid.git
   cd forvid
   npm install
   npm run tauri build
   ```

   Packages land in `src-tauri/target/release/bundle/` (`.AppImage`, `.deb`).

   On Linux, Forvid manages yt-dlp itself but expects `ffmpeg` from your
   distro (`sudo apt install ffmpeg`) — reliable static ffmpeg builds for
   Linux don't come as plain zips.

> Forvid is developed and tested on macOS; the same codebase builds on
> Windows and Linux.

## Using Forvid

1. **Copy a video or playlist link** anywhere (or paste one directly into
   the box). If the app notices a link on your clipboard, it offers to fill
   it in.
2. **Check the preview** — for a video: title, channel, duration and
   thumbnail. For a playlist: its title and video count, plus the folder it
   will create.
3. **Pick a quality** — "Best available", a specific resolution, or
   "Audio only · MP3". It defaults to a medium quality (~720p) to keep
   file sizes sane.
4. **(Optional) change where it saves** with the *Change* button next to
   "Save to".
5. **Hit Download.** Watch the live progress bar. Cancel anytime.
6. When it's done, **Reveal in Finder** takes you straight to the file.

> **Playlists** are saved into a new folder named after the playlist, inside
> your chosen destination — e.g. `~/Downloads/My Playlist/`.

> **Why does the progress bar sometimes fill twice?** For high-quality
> downloads, yt-dlp fetches the video and audio as separate streams and merges
> them with ffmpeg — one fill per stream. It's normal.

## Troubleshooting

**First-run setup fails**
Check your internet connection and hit *Try again* — the tool downloads
resume where they stopped (already-downloaded tools are kept). If a download
source is unreachable, installing the tools manually works too: Forvid
detects system installs (Homebrew, winget, distro packages) automatically.

**Downloads suddenly fail or YouTube errors appear**
YouTube changes its player regularly, and yt-dlp updates to keep up — an
outdated yt-dlp is the cause almost every time. If you have a system
install, update it (`brew upgrade yt-dlp` / `winget upgrade yt-dlp.yt-dlp`).
Your current yt-dlp version is shown in the bar at the bottom of the app.

**"Sign in to confirm you're not a bot"**
YouTube occasionally rate-limits. Waiting a while and updating yt-dlp usually
resolves it.

## Development

Built with **Tauri v2** (Rust backend) and **Next.js** (static export
frontend). The Rust core spawns yt-dlp and parses its documented machine
interfaces (`--progress-template`, `-j`, `--print after_move:filepath`) —
never its human-readable output.

```sh
# Prerequisites: Node.js, Rust (rustup), and yt-dlp + ffmpeg installed
# (any method — or let the app's first-run setup fetch them for you)

npm install

# Dev mode (hot reload; dev server on port 3100)
npm run tauri dev

# Rust engine tests (unit + arg-building + progress parsing)
cd src-tauri && cargo test

# Full end-to-end tests — hits the network, downloads a real video
cd src-tauri && cargo test -- --ignored

# Production bundle
npm run tauri build
```

### Project structure

```
app/                  Next.js frontend (static export)
  components/         Downloader, MetadataCard, ClipboardChip, StatusBar
lib/tauri.ts          Typed wrappers for every Tauri command/event
src-tauri/src/
  ytdlp.rs            Binary resolution, metadata fetch, env checks
  download.rs         Download engine: spawn, progress parse, cancel
  commands.rs         Tauri command surface
  models.rs           Types shared with the frontend (serde)
```

## Roadmap

- [ ] Download queue (line up several videos or playlists)
- [ ] Subtitle download options
- [ ] One-click updates for managed tools
- [ ] Per-video progress within a playlist download

## Acknowledgements

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — the downloader that does all
  the real work
- [Tauri](https://v2.tauri.app) — the native app shell
- [Next.js](https://nextjs.org) — the UI
