# Golden Launcher

Install, update, and open Chataigne, Chataigne 2, MapGyver, MapGyver 2, Blux, BlinderKitten, BenTo, and Le Grand Mechant Loop from one desktop library.

The library defaults to Stable. Enable Preview in an app’s release settings to include prereleases. Updates appear when the selected channel has a package for your device with a version newer than the verified installation. Installed apps can open offline, and cached release data is labeled.

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
