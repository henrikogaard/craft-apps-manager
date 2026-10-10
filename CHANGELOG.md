# Changelog

## 0.1.8

- New Craft apps show up by themselves. Every six hours, and on Check all, Craft Library looks at Storytold's GitHub for new `…craft` apps with a Mac release and lists them under All apps, marked New. They install, update, open and uninstall like the others. Source builds need a recipe, so those come in a later update.

## 0.1.7

- Command palette: ⌘K opens apps and runs actions like Update, Install, Check all and Settings. Search moves to /.
- Optional shortcut from any app (⌥Space, ⌃⌥Space or ⌥⌘Space) brings up Craft Library with the palette, and hides it again.
- Drop files on an installed app to open them in it.
- Drag apps in the sidebar to change their order; ⌘1–⌘9 follow it.
- Skip an update you don't want. It stops the reminder, the Dock count and automatic updates until the next version.
- Uninstall can also delete the app's settings, caches and logs, and separately its recovery files. The list of what goes comes from each app's source code, and user-made presets, swatches, signing IDs and media are kept.
- A welcome screen to pick apps when nothing is installed yet.
- Light mode checked across the new screens, and fixed hidden panels that could show up.

## 0.1.6

- Right-click an app for Open, Update, Show in Finder, Move, Check for update and Uninstall.
- Move between apps with the arrow keys and open the selected one with Return.
- Craft Library remembers its window size and position.
- Settings is split into General, Updates, Folders, Storage and Builds.
- Storage shows how much space build caches, builds, backups, downloads, sources and logs use, with buttons to clean them up.
- Optional automatic app updates after scheduled or startup checks, for apps that aren't open.
- Optional menu bar icon that lists your apps and waiting updates.
- Open Craft Library at login.
- Activity is now a history of what was installed, updated, moved and built, with the raw log underneath.
- Notifications say Craft Library instead of Craft Apps Manager.

## 0.1.5

- Choose where apps install. Pick another folder in Settings, and Craft Library offers to move the apps you already have there. Single apps can be moved from their details.
- Move the library (downloads, sources, builds, backups and logs) to another folder or drive. Saved paths and scheduled checks follow it, and Craft Library restarts when it's done. You can also point it at a library that's already somewhere else.
- Folders you used before are still searched, so apps left in them keep showing up and updating.

## 0.1.4

- Plainer wording across the app, and less of it. Removed the fork line from Settings and the slogans from the library, details panel and status bar.
- Rewrote the README.

## 0.1.3

- Right-click the Craft Library Dock icon to open any installed app; the Dock icon shows how many updates are waiting.
- Show which Craft apps are running, on cards, in the sidebar and in app details.
- Show “What’s new” for an available update, with a link to the full release notes.
- Open, configure launch options for, and delete local source builds from Builds.
- Move a leftover PrintCraft.app to the Trash once PdfCraft.app is installed beside it, and launch PdfCraft when old launch settings still name PrintCraft.
- Fix running-app detection, which never matched, so the “close the app first” checks now work.
- Report a late cancel or a real error correctly instead of always showing Cancelled (ported from upstream).

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
