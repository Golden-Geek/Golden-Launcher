# Golden Launcher progress

**83%** `████████░░` · 5 of 6 milestones complete

| # | Milestone | Status |
|---|---|---|
| 1 | Tauri, Svelte and TypeScript foundation with an eight-app catalog | Done |
| 2 | Publisher feeds, target resolution, versions, and offline cache | Done |
| 3 | Download verification, installation, detection, and launch | Done |
| 4 | Library, Updates, and inline operation controls | Done |
| 5 | Automated checks and six-target CI packaging | Done · packages verified for all six targets |
| 6 | Official install, update, and launch smoke tests on all six targets | Waiting on test devices · 2 of 6 targets passed |

Each milestone counts equally. The smoke-test milestone includes Raspberry Pi OS hardware for both architectures and a coexistence check once a qualifying MapGyver 2 package is available.

**CI packages** 100% `██████████` · 6 of 6 targets in one run. The Raspberry Pi packages were checked with `dpkg-deb`. [Desktop artifacts](https://github.com/Golden-Geek/Golden-Launcher/actions/runs/44917794553)

## Publisher smoke tests

**Install, update, and launch** 33% `███░░░░░░░` · 2 of 6 targets. [Publisher smoke run](https://github.com/Golden-Geek/Golden-Launcher/actions/runs/44914301865)

| Target | Official install → update → open |
|---|---|
| Windows x64 | Passed with official Chataigne 1.10.4 → 1.10.5b14 on a disposable runner. Interactive downloaded-file approval pending. |
| macOS Intel | Official package identity, version, and architecture verified. Interactive install, update, and launch pending. |
| macOS Apple Silicon | Official package identity, version, and architecture verified. Interactive install, update, and launch pending. |
| Linux x64 | Passed with official Chataigne 1.10.4 → 1.10.5b14 with Xvfb in CI and WSL, including the FUSE fallback. |
| Raspberry Pi OS aarch64 | Physical-device test pending |
| Raspberry Pi OS armhf | Physical-device test pending |

The Linux x64 Debian package also started the Golden Launcher window under Xvfb. Interactive macOS and Raspberry Pi OS testing is waiting on device access.

On each remaining desktop, start with no Chataigne installation. Install Chataigne from Stable, verify the installed version, then enable Preview and update. Verify that the installed version increased and that **Open** starts the app. Switch to Stable and confirm that the launcher offers **Open** with the installed Preview version. Disconnect the network, refresh to check the cached status, and open the app again.

Once a distinct MapGyver 2 package ships, install it alongside MapGyver, open both apps, then update MapGyver 2 and confirm that MapGyver still opens at its previous version.

## Window and version management

**75%** `████████░░` · 3 of 4 follow-up milestones complete

| Milestone | Status |
|---|---|
| Larger default window | Done · 1440 × 900 |
| Uninstall installed apps | Done · confirmation and verification after removal |
| Keep and open multiple installed versions | Done · per-version Open and Uninstall |
| Validate the updated launcher | In progress · Linux x64 opened both official versions and removed the older one |
