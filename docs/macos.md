# macOS

This fork targets macOS only. Apple silicon is tested locally; Intel packages can be built, but Intel runtime behavior remains unverified. Version 0.1.0 starts this fork’s release history.

## Installing apps

When an app publishes a `*-macos-universal.dmg`, the manager uses it for both release formats:

- **Installer** (default) copies the app into `/Applications`, or into `~/Applications` if `/Applications` is not writable. Apps you installed yourself are detected by their bundle name and `ai.storyteller.*` bundle identifier. Uninstall removes the app bundle.
- **Portable app** copies the app into the library under `releases/<app>/`.

Before installing, the manager checks GitHub's SHA-256 digest for the DMG. It then verifies the copied app with `codesign --verify --deep --strict` and Gatekeeper (`spctl --assess`).

The library is stored in `~/Library/Application Support/Craft Apps Manager`. Hourly checks use launch agents in `~/Library/LaunchAgents/io.github.craft-apps-manager.*.plist`, and notifications appear through Notification Center.

## Manager updates

Craft Library uses embedded Sparkle for manager updates. **Check for Updates…** in the native app menu opens Sparkle; Settings controls automatic checks and downloads. Active Craft operations block new manager checks and postpone an update relaunch until the operation finishes. Development executables outside a packaged bundle have no Sparkle controller.

The feed and public update-signing key are configured in `config/sparkle.json`. Tagged fork releases are signed with Developer ID, notarized by Apple and signed for Sparkle. See [release setup](releases.md) for workflow configuration and verification limits.

## Not yet available on macOS

- Deleting app profile data on uninstall. The upstream profile paths have not been verified yet.
- Shortcut files. App bundles open directly from Finder.

## Building

Install Rust with rustup and the Xcode Command Line Tools. Then build:

```text
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo test --locked -- --test-threads=1
./scripts/package-macos.sh
```

The script writes a universal `Craft Library.app` and a DMG with an Applications shortcut to `dist/macos`. The bundle has only an ad-hoc signature and is not notarized. The first time a downloaded copy is opened, macOS blocks it. Click **Done**, then go to **System Settings → Privacy & Security** and click **Open Anyway**. Right-click → **Open** no longer skips this check on macOS 15 and newer.

### Native source builds

The library lists all 14 apps: DesignCraft, EffectCraft, FilmCraft, LightCraft, PhotoCraft, PDFCraft, VectorCraft, WordCraft, GridCraft, DeckCraft, CADCraft, SoundCraft, ArtCraft and ArtCraft X.

**Install latest** checks the official `storytold/<app>` latest stable release. It prefers a universal Mac DMG, then a matching native DMG. If the repository has no release or the release has no compatible Mac asset, it sets up the needed tools, fetches upstream main, builds a native `.app`, and installs it. Network/authentication errors, failed checksums, invalid signatures and Gatekeeper rejection stop the operation; they never trigger a fallback that bypasses release verification. ArtCraft uses its own tag/asset naming and `ai.artcraft.app` identity. PDFCraft is fetched from `storytold/pdfcraft` and packaged as `PdfCraft.app`.

To build explicitly:

1. Select an app and choose **Set up build tools**. Rust and Xcode Command Line Tools are required. Homebrew supplies additional prerequisites when needed: `sevenzip`, PhotoCraft’s `libheif` and `pkg-config`, and ArtCraft’s Node.js/npm, CMake, NASM, LLVM and `pkg-config`. Tauri CLI 2 is installed into the manager’s tool folder. Missing Xcode tools or Homebrew produce an actionable error.
2. **Build .app** downloads source from the official GitHub repository at an exact commit. The 12 Rust apps use their upstream `packaging/macos/package.sh`; ArtCraft and ArtCraft X build their locked frontend and use Tauri to bundle a native Mac app. ArtCraft X is experimental and its upstream source may fail independently of the manager.
3. The manager verifies the completed bundle and records its exact app commit in `CraftManagerBuildCommit` and `build-info.json`. Compilation caches are retained by default; workspace cleanup is configurable in **Build settings**. Failed or cancelled builds retain their workspace and cache.
4. Close the Craft app and choose **Install built .app**. Existing apps keep their Applications location; new installs use `/Applications`, falling back to `~/Applications` when needed. Locally compiled apps are ad-hoc signed, not notarized releases.

Source archives and the `craft-fonts` dependency are fetched into the manager’s own library from official Storytold repositories. Existing preservation repositories and the obsolete `localRepositories` setting are ignored. The app commit does not identify changes in the separate fonts repository. **Check source commit** compares the installed commit with official upstream main, including changes with no version increase.

The manager embeds a local HTML/CSS interface through Wry and Tao. It has no remote UI or server dependency. Native macOS traffic lights, rounded window corners and a transparent title bar frame the custom interface. Graphite and light monochrome themes are persisted in `ui-settings.json`; app cards, details, builds, activity and settings share one window. The webview blocks remote navigation and sends only validated actions to the Rust backend. Hourly checks, app selection and build cleanup preferences are in Settings. The application menu includes a native About panel showing the icon, version, Henrik Øgård. Settings also provides About.

Native installed-app updates keep persistent backups under `backups/releases` when **Create app backups** is enabled. These directories include the signed bundle and `installed-app.json`; they stay uncompressed to preserve bundle contents and signatures. **Backups → Restore** verifies identity, signature and commit before replacing the app at its original Applications location. The selected backup is retained, together with a backup of the displaced app; restoration does not prune backups. Ordinary updates apply the configured retention count. Library installs use the same native bundle verification and restore transaction. Legacy library backups remain readable.

For deterministic local builds, the same backend can be run without opening the UI:

```sh
craft-apps-manager --root "/path/to/library" --build-app wordcraft --latest
craft-apps-manager --root "/path/to/library" --install-build wordcraft
craft-apps-manager --root "/path/to/library" --install-latest artcraft
```

With `--latest`, the source is fetched from official GitHub. Omit `--latest` to rebuild the previously downloaded, checksum-verified official archive. Installation requires a writable Applications folder or manager library, depending on Settings.

To package Manager for just the host architecture without installing the other Rust target, set `CRAFT_MANAGER_ARCH=arm64` (Apple silicon) or `x64` (Intel):

```sh
CRAFT_MANAGER_REPOSITORY=henrikogaard/craft-apps-manager CRAFT_MANAGER_ARCH=arm64 ./scripts/package-macos.sh
```
