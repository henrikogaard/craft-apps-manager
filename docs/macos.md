# macOS

The manager runs on macOS 11 or newer, on Apple silicon and Intel. It is experimental.

## Installing apps

When an app publishes a `*-macos-universal.dmg`, the manager uses it for both release formats:

- **Installer** (default) copies the app into `/Applications`, or into `~/Applications` if `/Applications` is not writable. Apps you installed yourself are detected by their bundle name and `ai.storyteller.*` bundle identifier. Uninstall removes the app bundle.
- **Portable app** copies the app into the library under `releases/<app>/`.

Before installing, the manager checks GitHub's SHA-256 digest for the DMG. It then verifies the copied app with `codesign --verify --deep --strict` and Gatekeeper (`spctl --assess`).

The library is stored in `~/Library/Application Support/Craft Apps Manager`. Hourly checks use launch agents in `~/Library/LaunchAgents/io.github.craft-apps-manager.*.plist`, and notifications appear through Notification Center.

## Manager updates

Craft Apps Manager.app can update itself. The new ZIP is checked against GitHub's SHA-256 digest, extracted into the library's `runtime/self-update` folder and verified with `codesign`. After the manager closes, the old app bundle is replaced. The previous bundle is kept in that folder as `previous.app`. The app must be in a folder you can write to, such as Applications. Development builds run outside a bundle and cannot update themselves.

Updates come from the GitHub repository set by `CRAFT_MANAGER_REPOSITORY` (owner/name) at build time. The default is `CryptoKey98/craft-apps-manager`. CI sets it to the repository being built, so a fork's packages update from the fork's releases.

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

The script writes a universal `Craft Apps Manager.app` and a ZIP to `dist/macos`. The bundle has only an ad-hoc signature and is not notarized. The first time a downloaded copy is opened, macOS blocks it. Click **Done**, then go to **System Settings → Privacy & Security** and click **Open Anyway**. Right-click → **Open** no longer skips this check on macOS 15 and newer.

Source builds use Homebrew for extra tools: `sevenzip`, plus `node`, `cmake` and `nasm` for ArtCraft X. WordCraft, GridCraft and DeckCraft use their upstream `packaging/macos/package.sh` to produce a native `.app` under `builds/`. Other apps still produce a bare executable.

### Native source builds

1. Open **Build from source → Settings** and enter an optional **Local repository parent folder**, such as `/Users/henrik/Dev/artcraft`. It should contain `wordcraft`, `gridcraft`, `deckcraft`, and optionally `craft-fonts`.
2. Select an app. **Check upstream source** compares its installed commit with upstream main, including changes that keep the same version number.
3. **Use latest source** builds upstream main. Unchecked uses the existing repository HEAD or the downloaded source ZIP. Only committed source is built: dirty and untracked files are preserved in the original repository. A separate clone is checked out for the build.
4. **Build app bundle** compiles with the upstream lockfile, packages fonts and licenses, verifies the bundle and records its exact commit. Build logs and cancellation use the existing job system. The compilation cache is retained by default on macOS; successful workspace cleanup is configurable. Failed and cancelled builds keep their workspace and cache.
5. Close the app and choose **Install built app**. It installs into the existing Applications folder, or `/Applications` with a fallback to `~/Applications` for new installs. Source builds are ad-hoc signed local builds, not notarized releases. Installer mode displays the installed version, channel and full commit.

A sibling `craft-fonts` folder is used when available. Otherwise the manager clones that support repository into its own workspace. The app commit does not identify changes in the separate fonts repository.

Native installed-app updates keep persistent backups under `backups/releases` when **Create app backups** is enabled. These directories include the signed bundle and `installed-app.json`; they stay uncompressed to preserve bundle contents and signatures. **Backups → Restore** verifies identity, signature and commit before replacing the app at its original Applications location. The selected backup is retained, together with a backup of the displaced app; restoration does not prune backups. Ordinary updates apply the configured retention count. Portable backups continue using their existing restore flow.

For deterministic local builds, the same backend can be run without opening the UI:

```sh
craft-apps-manager --root "/path/to/library" --build-app wordcraft --latest
craft-apps-manager --root "/path/to/library" --install-build wordcraft
```

The library must already have its local repository setting or downloaded source configured. Omit `--latest` to build the committed local source. Installation requires a writable Applications folder.

To package Manager for just the host architecture without installing the other Rust target, set `CRAFT_MANAGER_ARCH=arm64` (Apple silicon) or `x64` (Intel):

```sh
CRAFT_MANAGER_REPOSITORY=henrikogaard/craft-apps-manager CRAFT_MANAGER_ARCH=arm64 ./scripts/package-macos.sh
```
