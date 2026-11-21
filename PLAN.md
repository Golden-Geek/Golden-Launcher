# Golden Launcher

## Summary

Build a lightweight desktop launcher with Rust, Tauri 2, Svelte, and TypeScript. Users can install, update, and open Chataigne (v1), Chataigne 2, MapGyver, MapGyver 2, Blux, BlinderKitten, BenTo, and Le Grand Mechant Loop from a single library. [Tauri frontend guide](https://v2.tauri.app/start/frontend/)

Ship for Windows x64, macOS Intel and Apple Silicon, Linux x64, and Raspberry Pi OS Desktop on aarch64 and ARMv7 armhf. Show all eight apps on every platform, with availability based on the packages each publisher provides. [Tauri ARM packaging guide](https://v2.tauri.app/distribute/debian/) [Raspberry Pi OS downloads](https://www.raspberrypi.com/software/operating-systems/)

## Library and releases

- Use an original dark interface with charcoal surfaces, muted gold accents, rounded corners, subtle shadows, and existing app artwork. Show installed and available versions, release notes, and the relevant **Install**, **Update**, or **Open** action. Include an Updates view, a manual refresh, and inline progress and retry controls.
- Default each app to Stable. A per-app Preview setting includes versioned prereleases alongside stable builds. Offer the newest eligible build for the current platform and only offer updates above the verified installed version. Treat legacy `bN` versions as prereleases and `0.0.0` as a placeholder. Show preview-only titles with an option to enable Preview.
- Read the six existing publisher update feeds and confirm that each advertised package exists for the current target. Use public GitHub Releases from `Golden-Geek/Chataigne2` for both v2 apps. The feed is currently empty, so both start as **Awaiting release**. Match future assets by product, version, and target. Require MapGyver 2 packages to carry a distinct installed identity and destination because the current macOS configuration shares both with MapGyver v1. [Chataigne2 releases](https://github.com/Golden-Geek/Chataigne2/releases) [MapGyver downloads](https://github.com/Golden-Geek/MapGyver#downloads) [BlinderKitten downloads](https://blinderkitten.lighting/)

## Installation and app state

- Keep release resolution, downloads, install detection, process execution, and local state in Rust. Give Svelte typed commands for refresh, channel selection, install or update, cancel download, and launch. Rust resolves package URLs and installer arguments from the selected app and release.
- Stream packages to a temporary file over HTTPS from approved publisher hosts. Validate package type, target architecture where readable, and publisher SHA-256 when supplied. Run one install operation at a time. Allow cancellation during download and preserve native OS security and permission prompts. Verify the installed identity and version before reporting success.
- Detect existing Windows installations through known uninstall entries and executable metadata. Use quiet mode for verified Inno Setup and Tauri NSIS packages, with an explicit graphical retry and a restart-required status where applicable. On macOS, detect apps by bundle identity, hand `.pkg` files to Installer, and copy verified `.app` bundles from `.dmg` files into `~/Applications`. On Linux and Raspberry Pi, manage launcher-installed AppImages in a per-user directory and commit updates by atomic replacement. Provide the AppImage extract-and-run fallback when FUSE is unavailable and the package supports it. [Inno Setup options](https://jrsoftware.org/inno-setup/documentation/topic_setupcmdline.htm) [Apple Gatekeeper](https://support.apple.com/en-us/102445) [AppImage FUSE guidance](https://github.com/AppImage/docs.appimage.org/blob/master/source/user-guide/troubleshooting/fuse.rst)
- Cache release metadata and channel preferences locally. Keep installed apps launchable offline, mark cached metadata as stale, and rescan installed versions on startup and after each install attempt.

## Validation and packaging

- Test release parsing, channel selection, missing architectures, both v2 apps in the same release, and version comparisons. Exercise interrupted downloads, invalid packages, installer cancellation, denied permissions, and failed updates. Verify that the launcher continues to report the actual installed version after each failure.
- Produce CI artifacts as Windows NSIS, macOS DMG, Linux x64 AppImage and `.deb`, and Raspberry Pi `.deb` for aarch64 and armhf. Smoke-test on all six targets, including both Raspberry Pi OS architectures on hardware. Verify an official install, update, and launch on each target before public distribution. Include a MapGyver v1/v2 coexistence test when a qualifying v2 package is available.
