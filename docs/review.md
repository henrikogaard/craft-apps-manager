# macOS review evidence

Reviewed the native installation/build pipeline, filesystem boundaries, backups, settings, updater integration and macOS packaging during the 0.1.0 fork cleanup.

## Fixes

- Removed obsolete Windows/Linux installers, platform code, packaging and unused egui UI.
- Routed all 14 apps through official GitHub sources, preserving app-specific bundle names and identifiers.
- Fixed fresh source archive extraction into pre-existing workspaces and Applications/library backup restore destinations.
- Preserved entitlement metadata when signing upstream source bundles.
- Made backup selection use its managed name/source rather than a reorderable list index.
- Preserved independent source selections and rolled settings back if launch-agent configuration fails.
- Replaced the custom manager updater with embedded Sparkle; active operations block update checks and defer update relaunch.
- Added nested bundle signing, notarization, appcast generation and ephemeral CI credential cleanup.

## Evidence and limits

Final checks passed: 18 library tests, 2 interface tests and 20 workflow tests, plus Clippy, JavaScript syntax, shell syntax and release YAML parsing. The network release test is opt-in and was run separately. A live WordCraft official release download verified its digest, bundle identity, signature and Gatekeeper result. A live SoundCraft source build completed at commit c51e5d5ec52f11c6264b72e26661fa712feb5345. Local Developer ID signing verified the manager, Sparkle framework and nested helpers; appcast generation succeeded with the dedicated update key.

These checks do not establish that all 14 upstream projects compile, Intel runtime compatibility, every UI interaction, hosted release execution or a real Sparkle upgrade. ArtCraft/Tauri source recipes and a real older-to-newer update remain to be exercised. The final signed arm64 build was notarized, stapled, assessed by Gatekeeper, installed and launched. Dark and light appearance rendered; native About showed the app icon, version 0.1.0 and Henrik Øgård. Automated capture overlays obscure the traffic lights in the documentation screenshots.
