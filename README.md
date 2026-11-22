# Golden Launcher

Install, update, and open Chataigne, Chataigne 2, MapGyver, MapGyver 2, Blux, BlinderKitten, BenTo, Le Grand Mechant Loop, and SnoringPony from one desktop library.

[Download Golden Launcher](https://github.com/Golden-Geek/Golden-Launcher/releases/latest) for Windows x64 or ARM64, macOS Intel or Apple Silicon, Linux x64, or Raspberry Pi OS. On macOS, open the DMG and drag Golden Launcher to Applications.

Golden Launcher checks for its own updates on startup, while it is running, and when you press **F5**. It downloads and verifies newer releases in the background. Choose **Install and restart** to apply an update. Debian packages can ask for system administrator approval. On Windows 11 ARM64, the library prefers native ARM64 packages when a publisher provides both builds for the same version and can run x64 packages through Windows emulation.

For a fresh install, select an app to see the latest compatible **Stable** and **Beta** builds side by side. Use the version picker to browse compatible older releases and read their release notes. The launcher marks releases newer than the verified installation as updates. Installed apps can open offline, and cached release data is labeled.

Use **Manage versions** on an app card to open or uninstall a specific installation. On Linux and Raspberry Pi, AppImage updates keep earlier versions, and **Install alongside** adds a different eligible version. The launcher asks for confirmation before running an installer that would replace a newer native version. On Windows and macOS, choose **Keep a copy** to preserve the installed version in the launcher. Copies can share the publisher’s app settings and may need the other version closed before launch.

## Development

Install Node.js 24, Rust 1.95 or later, and the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your operating system.

```sh
npm ci
npm run tauri dev
```

Run the type checks and Rust tests locally.

```sh
npm run check
npm run build
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

## Packages

The Desktop packages workflow uploads Windows x64 and ARM64 NSIS, macOS Intel and Apple Silicon DMGs, Linux x64 AppImage and Debian packages, and Raspberry Pi OS aarch64 and armhf Debian packages. See the [plan](PLAN.md) and [progress report](PROGRESS.md).

To release Golden Launcher, set the same version in `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, and `src-tauri/tauri.conf.json`. Push an annotated `vMAJOR.MINOR.PATCH` tag with the release notes. The Desktop packages workflow signs the packages with the repository's `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets, verifies the signatures, and stages the downloads and `latest.json` as a draft. Publish the draft as the latest GitHub release to make the updates available.

### Publishing Chataigne 2 and MapGyver 2

The launcher reads public releases from [Golden-Geek/Chataigne2](https://github.com/Golden-Geek/Chataigne2/releases). Use a versioned tag such as `v2.0.0-beta.1` and include the product, matching version, and target in each asset name. For example, `Chataigne2_2.0.0-beta.1_x64-setup.exe` is a Windows asset and `MapGyver2_2.0.0-beta.1_aarch64.dmg` is an Apple Silicon asset. A release may include assets for both apps.

For MapGyver 2 on macOS, the disk image must contain a bundle with identifier `com.goldengeek.mapgyver2`, which the launcher installs as `~/Applications/MapGyver2.app`. On Windows the setup executable must identify itself as MapGyver 2. The launcher gives it a separate installation directory.
