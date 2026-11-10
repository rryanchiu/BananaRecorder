<div align="center">
  <img src="public/banana.png" width="80" alt="BananaRecorder logo" />
  <h1>BananaRecorder</h1>
  <p><strong>Go live. Keep the recording.</strong><br />A multi-platform live stream monitoring and recording app for Windows.</p>
  <p>
    <img src="https://img.shields.io/badge/platform-Windows-0078D4" alt="Windows" />
    <img src="https://img.shields.io/badge/Tauri-2-24C8D8" alt="Tauri 2" />
    <img src="https://img.shields.io/badge/Rust-native-CE422B" alt="Rust backend" />
    <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green" alt="MIT license" /></a>
  </p>
  <p><a href="README.md">简体中文</a> · <strong>English</strong></p>
  <p><a href="#features">Features</a> · <a href="#getting-started">Getting started</a> · <a href="#local-development">Development</a> · <a href="#building-installers">Packaging</a> · <a href="#license-and-acknowledgments">License</a></p>
</div>

---

BananaRecorder brings room management, live detection, FFmpeg recording, and video organization into one desktop window. React powers the interface and Rust runs the backend, with no Python service required.

> The current version is **0.1.0**, targeting Windows. The application UI is currently in Simplified Chinese; this English README does not imply an English UI is available. Platform APIs may change. Listed support means a resolver is implemented, not that every room is always accessible.

## Features

| Capability | Details |
| --- | --- |
| Multi-platform management | Manage Douyin, Bilibili, Huya, and Kuaishou rooms with individual enable switches and quality settings |
| Automatic monitoring | Check rooms periodically, record when streams go live, and check again after interruptions |
| Flexible recording | TS / MKV / FLV / MP4 output, timed segments, and folders organized by creator and date |
| Post-processing | Automatic MP4 conversion with optional source deletion; batch conversion and merging in the library |
| Video library | Browse, preview, save copies, delete recordings, and open their folders |
| Live dashboard | Monitor room counts, active recordings, available disk space, and runtime logs |
| Desktop integration | System tray, single-instance behavior, custom window controls, and internal scrolling |
| Connection settings | Douyin/Bilibili cookies and proxy configuration |

## Supported platforms

Use a live room URL rather than a shortened share link or a general profile page.

| Platform | URL format | Notes |
| --- | --- | --- |
| Douyin | `https://live.douyin.com/ROOM_ID` | Short links and general profile URLs are not currently supported |
| Bilibili | `https://live.bilibili.com/ROOM_ID` | Cookies can be configured in settings |
| Huya | `https://www.huya.com/ROOM_ID_OR_NAME` | Available quality depends on the stream |
| Kuaishou | `https://live.kuaishou.com/u/CREATOR_NAME` | Uses live page parsing and may be affected by access restrictions |

## Getting started

### Install and launch

If this repository has published Releases, choose a Windows installer. Otherwise, build from source using the instructions below. An installer is preferable to distributing the main executable alone because recording also requires FFmpeg.

Runtime requirements: Windows 10 / 11 and Microsoft Edge WebView2 Runtime. Development and packaging currently target Windows; macOS and Linux validation is not documented.

### Your first recording

1. Open **Recording settings (录制设置)** and choose a destination, quality, format, and segmentation options.
2. Add a live room in **Room management (直播间管理)** and enable its switch.
3. Click **Start monitoring (开始监控)** in the top-right corner. Recording starts automatically when an enabled room is live.
4. Check progress and logs in **Dashboard (仪表盘)** and browse recordings in **Video library (视频库)**.

**Monitoring and stop behavior:**

- **Stop monitoring (停止监控)** stops the global monitoring engine and active recording tasks.
- **A room's stop button** stops that recording and disables the room. Enable its switch again to resume monitoring.
- **Closing the window** hides the app to the system tray while background tasks continue. Use the tray menu to quit completely.
- **Relaunching the app** automatically starts the monitoring engine if saved rooms are enabled.

### Default settings

| Setting | Default |
| --- | --- |
| Quality | Original (原画) |
| Output format | TS |
| Check interval | 120 seconds |
| Segmentation | Enabled, 30 minutes per segment |
| Convert to MP4 after recording | Enabled |
| Delete source after conversion | Enabled |
| Folder organization | By creator |

Review these options before recording, especially source deletion after conversion. Available quality depends on the platform, account permissions, and stream.

## Data and privacy

On Windows, settings and the room list are stored by default in:

```text
%APPDATA%\com.ryan.liverecorder\
├── settings.json
└── rooms.json
```

When no output directory is configured, recordings go into `downloads` under the application data directory. Moving the source checkout does not move this user data.

Cookies are stored in the local settings JSON, not in an encrypted credential store. Do not publish settings files, cookies, authenticated stream URLs, or unredacted logs in commits, issues, or screenshots.

## Local development

### Prerequisites

- **Node.js 22.12+** and npm. The installed Vite version requires `^20.19.0 || >=22.12.0`.
- **Rust stable** with the Windows MSVC toolchain.
- **Visual Studio Build Tools**, including Desktop development with C++ and the Windows SDK.
- **Microsoft Edge WebView2 Runtime**.
- **FFmpeg (full build only)**: place a Windows build at `src-tauri/binaries/ffmpeg.exe`, or `src-tauri/binaries/ffmpeg` on macOS/Linux.

> The FFmpeg binary is not tracked in Git. The full build bundles it; the lite build does not, and does not require the resource to compile. Users can install FFmpeg from **Settings → FFmpeg** or point the app at an existing binary. Review the license of the build you ship.

### Install dependencies and run

After cloning or downloading this repository, run from the project root:

```powershell
npm ci
npm run tauri dev
```

The running app does not require Python or the original DouyinLiveRecorder checkout. `npm run dev` starts only the frontend server; use `npm run tauri dev` for full desktop functionality.

### Common commands

| Command | Purpose |
| --- | --- |
| `npm run tauri dev` | Run the desktop development environment |
| `npm run build` | TypeScript checking and frontend production build |
| `cargo check --manifest-path src-tauri/Cargo.toml` | Check the Rust backend |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Run default tests |
| `npm run tauri build` | Build the full app and installers (FFmpeg bundled) |
| `npm run tauri:build:lite` | Build the lite app (no bundled FFmpeg) |
| `npm run tauri:dev:lite` | Run the dev environment with the lite config |

## Building installers

From the project root:

```powershell
npm run tauri build
```

Main output locations:

```text
src-tauri/target/release/
├── liverecorder.exe       # Main executable; the internal name is still liverecorder
└── bundle/
    ├── nsis/             # Windows EXE installer
    └── msi/              # MSI installer, when that packaging step succeeds
```

To build only the NSIS installer:

```powershell
npm run tauri build -- --bundles nsis
```

### Full and lite builds

- **Full**: `npm run tauri build` bundles the FFmpeg binary from `src-tauri/binaries/`.
- **Lite**: `npm run tauri:build:lite` ships without FFmpeg, producing a smaller installer
  under the product name `BananaRecorder Lite` so it does not overwrite the full build.
  After first launch, open **Settings → FFmpeg** and click **Install FFmpeg**, or point it
  at an existing binary.

FFmpeg tab actions:

| Action | Behavior |
| --- | --- |
| Use bundled version | Clears the custom path; auto order is bundled → in-app install → PATH |
| Use local path | File picker for an existing `ffmpeg` / `ffmpeg.exe` |
| Install FFmpeg | Downloads into the app data directory (no admin rights) and enables it |

The main EXE is not a complete, self-contained single-file distribution. Manual distribution must also account for FFmpeg, WebView2, and third-party license files. The first build may download Rust dependencies and installer tooling.

## Publishing a release

1. **Bump the version** in `src-tauri/tauri.conf.json` (and `package.json` for consistency).
2. **Tag and push**:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

3. **Wait for Actions**: `.github/workflows/release.yml` downloads the bundled LGPL FFmpeg,
   builds the Windows NSIS installer and a macOS dmg (unsigned), and creates a
   **draft release** with the installers and generated release notes.
4. **Publish**: review the draft on GitHub Releases and click Publish.

Notes:

- The app queries GitHub `releases/latest`, which returns only **published** releases
  (drafts and prereleases are ignored), so users are not prompted before you publish.
- The tag must match the `version` in `tauri.conf.json`, otherwise the workflow fails.
- The workflow ships the full build only; build the lite variant locally with
  `npm run tauri:build:lite` and upload it manually if needed.
- The macOS build is unsigned; users must right-click → Open the first time. Add signing
  and notarization before wider distribution.
- The current upgrade button opens the installer download; true one-click updates need
  `tauri-plugin-updater` with signed artifacts and a generated `latest.json`.
- No extra Secrets are required for this flow; it uses the repository's `GITHUB_TOKEN`
  with `contents: write`.

## Testing

Default tests cover FFmpeg command construction, filename sanitization, signature algorithms, and Python/Rust signature test vectors. Live network tests are skipped by default.

```powershell
# Default tests
cargo test --manifest-path src-tauri/Cargo.toml

# Explicit live tests: access real platforms; some tests make a short recording
cargo test --manifest-path src-tauri/Cargo.toml -- --ignored --nocapture
```

Live tests depend on current room state, platform APIs, and network conditions. They are not reproducible offline checks.

Reference signature vectors are committed to the repository. Only regenerating them requires the original Python project and its dependencies:

```powershell
python src-tauri/tests/gen_vectors.py "D:\path\to\DouyinLiveRecorder"
```

## Project structure

```text
src/
├── components/           # Window layout and shared UI
├── pages/                # Dashboard, rooms, library, and settings
└── lib/                  # Tauri calls and frontend state
src-tauri/
├── src/
│   ├── resolver/         # Platform stream resolution and signatures
│   ├── recorder/         # Recording scheduler and FFmpeg processes
│   ├── commands.rs       # Frontend/backend command interface
│   ├── config.rs         # Settings persistence
│   └── rooms.rs          # Room management
├── tests/                # Unit/integration tests and reference vectors
├── binaries/             # Locally supplied FFmpeg; excluded from Git
└── tauri.conf.json       # Window and packaging configuration
```

## Troubleshooting

**Monitoring is running, but nothing is recording.**

Confirm that the room is enabled and live, then inspect the dashboard logs. Unsupported URL formats, expired cookies, networking issues, or platform restrictions can prevent resolution.

**A recording will not play inside the app.**

Playback depends on the containers and codecs supported by WebView2. Convert to MP4 or use a local media player; changing the container alone does not guarantee codec compatibility.

**FFmpeg cannot be found.**

The app checks PATH, the development resource directory, `ffmpeg.exe` beside the main executable, and the installed resource directory, in that order. For development, verify that `src-tauri/binaries/ffmpeg.exe` exists.

**Code changes do not appear in the installed app.**

Use `npm run tauri dev` for development. An installed version must be rebuilt and updated.

## Contributing

Bug reports, platform resolver fixes, and usability improvements are welcome. Include the app version, Windows version, reproduction steps, relevant redacted logs, and the affected platform. Discuss larger changes in an issue first.

Before submitting changes, run the frontend build and relevant Rust tests. For resolver changes, describe the platforms and scenarios verified, along with any remaining gaps. Do not commit dependencies, build artifacts, recordings, or personal configuration.

## License and acknowledgments

Parts of BananaRecorder's signature algorithms and platform resolution logic are **ported or derived from** [DouyinLiveRecorder](https://github.com/ihmily/DouyinLiveRecorder) (MIT, Copyright © 2025 Hmily). These include Douyin a_bogus signing, Huya anti-leech signing, and portions of stream resolution. Their attribution and license notices are retained; these portions are not claimed as entirely original work.

Thanks to Tauri, Rust, React, FFmpeg, shadcn/ui, Radix UI, Tailwind CSS, Lucide, and Sonner.

The project's own code is licensed under the [MIT License](LICENSE). Third-party code and binaries retain their respective licenses; see [Third-party notices](THIRD-PARTY-NOTICES.md). The current local FFmpeg binary is `6.0-essentials_build-www.gyan.dev`, built with GPL/version3 enabled. Before distributing installers containing FFmpeg, verify and provide the license texts and corresponding-source arrangements required for that build. The project's MIT license does not replace FFmpeg's license obligations.

Only record and use content you are entitled to save, and follow applicable platform rules and laws.
