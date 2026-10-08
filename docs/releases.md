# Signed macOS releases and Sparkle

Craft Library embeds Sparkle 2.10.0, pinned by SHA-256 in `config/sparkle.json`. The native Check for Updates menu opens Sparkle. Settings controls automatic checks and downloads; packaged builds enable both by default. Craft app installations use their separate official Storytold release/source pipeline.

## Release workflow

Push a version tag matching Cargo.toml, such as `v0.1.0`, after reviewing the release commit. `.github/workflows/release.yml` runs formatting, tests and Clippy, builds for Apple Silicon and Intel, signs nested helpers/frameworks and the app with Developer ID and hardened runtime, submits the ZIP to Apple, staples the accepted ticket, verifies the signature and Gatekeeper, and generates an Ed25519-signed Sparkle appcast. It publishes the ZIP, appcast.xml and SHA256SUMS in that tag's GitHub Release.

The feed is `https://github.com/henrikogaard/craft-apps-manager/releases/latest/download/appcast.xml`. It is unavailable until the first release is published. Every subsequent release must have a greater CFBundleVersion; packaging currently uses the Cargo version for both display and bundle versions. Preserve the bundle identifier and Sparkle key pair across releases. A manager repository override does not redirect Craft app repositories; configure the feed separately when distributing another fork.

## GitHub configuration

| Secret | Encoding and purpose |
| --- | --- |
| DEVELOPER_ID_P12_BASE64 | Base64 Developer ID Application certificate/private-key export |
| DEVELOPER_ID_P12_PASSWORD | Password protecting that export |
| APPSTORE_API_PRIVATE_KEY | Base64 App Store Connect team API private key (.p8) for notarization |
| SPARKLE_PRIVATE_ED_KEY | Sparkle generate_keys export, passed through a private file |

Variables: APPSTORE_API_KEY_ID, APPSTORE_API_ISSUER_ID, SPARKLE_PUBLIC_ED_KEY, SPARKLE_FEED_URL. Public Sparkle configuration is also stored in config/sparkle.json. Private credentials stay outside Git; CI imports them into an ephemeral keychain/private directory and removes them in an always-run cleanup step.

On 2026-10-08 all four secrets and four variables were configured by metadata readback. A dedicated Craft Library team API key successfully authenticated to Apple's notarization service. Local Developer ID signing, Apple notarization, stapling, Gatekeeper assessment and Sparkle appcast generation passed on the arm64 0.1.0 package. The same signed app was installed and launched from /Applications. These checks do not prove hosted release execution or a real older-to-newer Sparkle upgrade. No fork release has been published yet.

## Local packaging

The package script defaults to ad-hoc signing for development. Set CRAFT_SIGN_IDENTITY to a Developer ID Application identity for distribution signing. `scripts/release-macos.sh` requires that identity, APPSTORE_API_KEY_FILE, APPSTORE_API_KEY_ID, APPSTORE_API_ISSUER_ID, SPARKLE_PRIVATE_KEY_FILE and CRAFT_RELEASE_TAG. It builds a universal archive and performs notarization and appcast generation without publishing it.

Sources: [Sparkle setup](https://sparkle-project.org/documentation/), [programmatic setup](https://sparkle-project.org/documentation/programmatic-setup/), [publishing updates](https://sparkle-project.org/documentation/publishing/), [Apple notarization](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution).
