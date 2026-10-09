# Craft Library

<img src="assets/craft-library-icon-rounded.png" width="88" alt="Craft Library icon">

A Mac app for installing, updating and opening the [Storytold](https://github.com/orgs/storytold/repositories) Craft apps: LightCraft, PhotoCraft, WordCraft and the rest. It works as a launcher too. macOS only.

![Craft Library in dark mode](docs/images/library-dark.png)

## Install

Download the DMG from [releases](https://github.com/henrikogaard/craft-apps-manager/releases), open it and drag Craft Library into Applications. Releases are signed and notarized, and the app updates itself through Sparkle.

## What it does

- Installs the official Mac release of each app. If an app has no Mac release yet, it builds one from the official source instead.
- Shows the version you have next to the latest one, with the release notes. Check all looks for updates, Update all installs them.
- Opens apps. Double-click a card, press ⌘1 to ⌘9 for the apps in the sidebar, or press ⌘K, type a name and hit Return. You can also right-click the Dock icon and pick an app from there.
- Shows which apps are running, and puts the number of waiting updates on the Dock icon.
- Can keep the previous version when it updates an app, so you can roll back.
- Builds apps from source and lets you open a build without installing it.

Everything comes from the official `storytold/<app>` repositories. A failed checksum, signature or Gatekeeper check stops the install. It never falls back to a source build to get around that.

The 14 apps are DesignCraft, EffectCraft, FilmCraft, LightCraft, PhotoCraft, PDFCraft, VectorCraft, WordCraft, GridCraft, DeckCraft, CADCraft, SoundCraft, ArtCraft and ArtCraft X. ArtCraft X is experimental and has no release yet.

## Building apps from source

You need Rust and the Xcode Command Line Tools. **Set up tools** in an app's Source build section installs the rest through Homebrew: libheif for PhotoCraft, and Node.js, CMake, NASM, LLVM, pkg-config and Tauri CLI 2 for ArtCraft.

Local builds are ad-hoc signed, so they won't pass Gatekeeper like a release does. Whether a build succeeds depends on the upstream project compiling on your machine.

There's also a command line for scripting:

```sh
craft-apps-manager --build-app wordcraft --latest
craft-apps-manager --install-build wordcraft
craft-apps-manager --install-latest artcraft
```

Leave out `--latest` to build from the source archive you already downloaded. `--root /some/path` uses a different library folder.

## Where things go

The library lives in `~/Library/Application Support/Craft Apps Manager`:

```text
releases/    downloaded releases
sources/     source archives and the commit each one came from
builds/      finished source builds
workspace/   extracted source, build tools and caches
backups/     previous app versions
logs/        install, build and check logs
```

You can put the library somewhere else, an external drive for example, in Settings. Craft Library moves everything over and restarts. Apps install to /Applications unless you pick another folder there, and it offers to move the apps you already have.

## Building Craft Library itself

It's Rust. The window is a local HTML page (`src/web/index.html`) shown through [Wry](https://github.com/tauri-apps/wry) and [Tao](https://github.com/tauri-apps/tao). Nothing loads from the network.

```sh
cargo test --locked -- --test-threads=1
CRAFT_MANAGER_ARCH=arm64 ./scripts/package-macos.sh
```

That writes `Craft Library.app` and a DMG to `dist/macos`. Use `x64` for Intel, or leave `CRAFT_MANAGER_ARCH` out for a universal build (needs both Rust Mac targets).

Pushing a `v*` tag that matches `Cargo.toml` makes a signed, notarized release. [docs/releases.md](docs/releases.md) has the details, [docs/macos.md](docs/macos.md) covers signing and source builds, and [docs/architecture.md](docs/architecture.md) explains the layout.

## Credits and license

Craft Library is a fork of [Craft Apps Manager](https://github.com/CryptoKey98/craft-apps-manager) by CryptoKey98 and contributors. Fork changes by Henrik Øgård.

MIT license, with both copyright notices kept in [LICENSE](LICENSE). The Craft apps and their icons belong to Storytold. App icons and fonts have their own notices in [THIRD-PARTY-NOTICES.txt](THIRD-PARTY-NOTICES.txt).
