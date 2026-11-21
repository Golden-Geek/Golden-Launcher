# Golden Launcher

Install, update, and open Chataigne, Chataigne 2, MapGyver, MapGyver 2, Blux, BlinderKitten, BenTo, and Le Grand Mechant Loop from one desktop library.

The library defaults to Stable. Enable Preview in an app’s release settings to include prereleases. Updates appear when the selected channel has a package for your device with a version newer than the verified installation. Installed apps can open offline, and cached release data is labeled.

Use **Manage versions** on an app card to open or uninstall a specific installation. On Linux and Raspberry Pi, AppImage updates keep earlier versions. Switch channels and choose **Install alongside** to add a different eligible version. On Windows and macOS, choose **Keep a copy** before updating to preserve the installed version in the launcher. Copies can share the publisher’s app settings and may need the other version closed before launch.

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

The Desktop packages workflow uploads Windows x64 NSIS, macOS Intel and Apple Silicon DMGs, Linux x64 AppImage and Debian packages, and Raspberry Pi OS aarch64 and armhf Debian packages. CI artifacts are for verification. See the [plan](PLAN.md) and [progress report](PROGRESS.md).

### Publishing Chataigne 2 and MapGyver 2

The launcher reads public releases from [Golden-Geek/Chataigne2](https://github.com/Golden-Geek/Chataigne2/releases). Use a versioned tag such as `v2.0.0-beta.1` and include the product, matching version, and target in each asset name. For example, `Chataigne2_2.0.0-beta.1_x64-setup.exe` is a Windows asset and `MapGyver2_2.0.0-beta.1_aarch64.dmg` is an Apple Silicon asset. A release may include assets for both apps.

For MapGyver 2 on macOS, the disk image must contain a bundle with identifier `com.goldengeek.mapgyver2`, which the launcher installs as `~/Applications/MapGyver2.app`. On Windows the setup executable must identify itself as MapGyver 2. The launcher gives it a separate installation directory.
