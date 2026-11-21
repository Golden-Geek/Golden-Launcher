# Golden Launcher progress

**83%** `████████░░` · 5 of 6 milestones complete

| # | Milestone | Status |
|---|---|---|
| 1 | Tauri, Svelte and TypeScript foundation with an eight-app catalog | Done |
| 2 | Publisher feeds, target resolution, versions, and offline cache | Done |
| 3 | Download verification, installation, detection, and launch | Done |
| 4 | Library, Updates, and inline operation controls | Done |
| 5 | Automated checks and six-target CI packaging | Done · packages verified for all six targets |
| 6 | Official install, update, and launch smoke tests on all six targets | Waiting on test devices · 4 of 6 targets passed |

Each milestone counts equally. The smoke-test milestone includes Raspberry Pi OS hardware for both architectures and a coexistence check once a qualifying MapGyver 2 package is available.

**CI packages** 100% `██████████` · 6 of 6 targets in one run. The Raspberry Pi packages were checked with `dpkg-deb`. [Desktop artifacts](https://github.com/Golden-Geek/Golden-Launcher/actions/runs/44920315991)

## Publisher smoke tests

**Install, update, and launch** 67% `███████░░░` · 4 of 6 targets. [Publisher smoke run](https://github.com/Golden-Geek/Golden-Launcher/actions/runs/44920316234)

| Target | Official install → update → open |
|---|---|
| Windows x64 | Passed with official Chataigne 1.10.4 → 1.10.5b14 on a disposable runner. Both versions opened, then the saved copy was removed. Interactive downloaded-file approval pending. |
| macOS Intel | Passed with official Chataigne 1.10.4 → 1.10.5b14 on a hosted runner using the system Installer command. Both versions launched. Graphical Installer approval pending. |
| macOS Apple Silicon | Passed with official Chataigne 1.10.4 → 1.10.5b14 on a hosted runner using the system Installer command. Both versions launched. Graphical Installer approval pending. |
| Linux x64 | Passed with official Chataigne 1.10.4 → 1.10.5b14 with Xvfb in CI and WSL, including the FUSE fallback. Both versions opened, then the older version was removed. |
| Raspberry Pi OS aarch64 | Physical-device test pending |
| Raspberry Pi OS armhf | Physical-device test pending |

The Linux x64 Debian package also started the Golden Launcher window under Xvfb. The remaining device checks cover the graphical macOS Installer and both Raspberry Pi OS Desktop architectures on hardware.

On a clean test desktop, install Chataigne from Stable and verify the installed version. Enable Preview and update. Where native approval is required, cancel once and verify that the installed version stays the same, then retry and approve. Verify that the installed version increased and that **Open** starts the app. Switch to Stable and confirm that the launcher offers **Open** with the installed Preview version. Disconnect the network, refresh to check the cached status, and open the app again.

Once a distinct MapGyver 2 package ships, install it alongside MapGyver, open both apps, then update MapGyver 2 and confirm that MapGyver still opens at its previous version.

## Window and version management

**100%** `██████████` · 4 of 4 follow-up milestones complete

| Milestone | Status |
|---|---|
| Larger default window | Done · 1440 × 900 |
| Uninstall installed apps | Done · confirmation and verification after removal |
| Keep and open multiple installed versions | Done · per-version Open and Uninstall |
| Validate the updated launcher | Done · Windows WebView2 controls, Windows and Linux publisher smoke tests, retained bundle tests on both macOS targets, and all six CI packages |
