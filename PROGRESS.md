# Golden Launcher progress

**67%** `███████░░░` · 4 of 6 milestones complete

| # | Milestone | Status |
|---|---|---|
| 1 | Tauri, Svelte and TypeScript foundation with an eight-app catalog | Done |
| 2 | Publisher feeds, target resolution, versions, and offline cache | Done |
| 3 | Download verification, installation, detection, and launch | Done |
| 4 | Library, Updates, and inline operation controls | Done |
| 5 | Automated checks and six-target CI packaging | In progress · workflow added |
| 6 | Official install, update, and launch smoke tests on all six targets | In progress · Linux x64 verified with Xvfb |

Each milestone counts equally. The smoke-test milestone includes Raspberry Pi OS hardware for both architectures and a coexistence check once a qualifying MapGyver 2 package is available.

## Publisher smoke tests

| Target | Official install → update → open |
|---|---|
| Windows x64 | Hosted smoke test prepared |
| macOS Intel | Native package preflight prepared. Interactive install and launch pending. |
| macOS Apple Silicon | Native package preflight prepared. Interactive install and launch pending. |
| Linux x64 | Passed with official Chataigne 1.10.4 → 1.10.5b14 in an Xvfb desktop session, including the FUSE fallback. |
| Raspberry Pi OS aarch64 | Physical-device test pending |
| Raspberry Pi OS armhf | Physical-device test pending |

On each remaining desktop, start with no Chataigne installation. Install Chataigne from Stable, verify the installed version, then enable Preview and update. Verify that the installed version increased and that **Open** starts the app. Switch to Stable and confirm that the launcher offers **Open** with the installed Preview version. Disconnect the network, refresh to check the cached status, and open the app again.

Once a distinct MapGyver 2 package ships, install it alongside MapGyver, open both apps, then update MapGyver 2 and confirm that MapGyver still opens at its previous version.
