# Golden Launcher progress

**67%** `███████░░░` · 4 of 6 milestones complete

| # | Milestone | Status |
|---|---|---|
| 1 | Tauri, Svelte and TypeScript foundation with an eight-app catalog | Done |
| 2 | Publisher feeds, target resolution, versions, and offline cache | Done |
| 3 | Download verification, installation, detection, and launch | Done |
| 4 | Library, Updates, and inline operation controls | Done |
| 5 | Automated checks and six-target CI packaging | In progress · packages uploaded for 4 of 6 targets |
| 6 | Official install, update, and launch smoke tests on all six targets | In progress · Linux x64 verified with Xvfb |

Each milestone counts equally. The smoke-test milestone includes Raspberry Pi OS hardware for both architectures and a coexistence check once a qualifying MapGyver 2 package is available.

[Desktop packaging run](https://github.com/Golden-Geek/Golden-Launcher/actions/runs/44913372437)

## Publisher smoke tests

| Target | Official install → update → open |
|---|---|
| Windows x64 | Disposable-runner smoke test in progress. Interactive downloaded-file approval pending. |
| macOS Intel | Official package identity, version, and architecture verified. Interactive install, update, and launch pending. |
| macOS Apple Silicon | Official package identity, version, and architecture verified. Interactive install, update, and launch pending. |
| Linux x64 | Passed with official Chataigne 1.10.4 → 1.10.5b14 in an Xvfb desktop session, including the FUSE fallback. |
| Raspberry Pi OS aarch64 | Physical-device test pending |
| Raspberry Pi OS armhf | Physical-device test pending |

On each remaining desktop, start with no Chataigne installation. Install Chataigne from Stable, verify the installed version, then enable Preview and update. Verify that the installed version increased and that **Open** starts the app. Switch to Stable and confirm that the launcher offers **Open** with the installed Preview version. Disconnect the network, refresh to check the cached status, and open the app again.

Once a distinct MapGyver 2 package ships, install it alongside MapGyver, open both apps, then update MapGyver 2 and confirm that MapGyver still opens at its previous version.
