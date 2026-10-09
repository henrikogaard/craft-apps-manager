# Changelog

## 0.1.2

- Fix updates failing with “Invalid app signature” when the installed app carries Finder info. Copied bundles now drop all extended attributes before the strict signature check.
- Show why a signature check failed in the activity log.
- Show an updated app as current right after Update or Update all.

## 0.1.1

- Ship a signed and notarized DMG with an Applications shortcut instead of a ZIP.
- Rename the app bundle and download to Craft Library.

## 0.1.0 — macOS fork

- Start Henrik Øgård’s macOS-only fork of CryptoKey98’s Craft Apps Manager.
- Add a local monochrome interface with dark/light modes, native traffic lights, rounded window corners and About panel.
- Install official Mac releases, or build official source into native bundles when no compatible release exists; include all 14 Craft apps.
- Record source provenance and preserve verified bundle backups and rollback.
- Remove Windows/Linux packaging, platform implementations and the unused egui interface.
- Open the library on My apps, sorted by most recently opened. Double-click an app, press ⌘1–⌘9, ⌘O, or search with ⌘K and press Return to open it.
- Show installed and latest versions on cards and in app details, with the time of the last check.
- Add Check all and Update all for installed apps.
- Align the sidebar, library and details headers.

For the original project’s history, see the [upstream changelog](https://github.com/CryptoKey98/craft-apps-manager/blob/main/CHANGELOG.md).
