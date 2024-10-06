# BYBE-Portable
Multi-platform desktop client for the BYBE website

## Build locally
In the project directory, run
```bash
cargo tauri build
```
Errors should only happen if you have a Arch/Fedora distribution and AppImage as target build.
(https://tauri.app/v1/guides/building/linux/#limitations / https://github.com/linuxdeploy/linuxdeploy/issues/272)
If there are errors while building the AppImage, first delete the target folder at ./BYBE-tauri/target and when there is no target folder run
```bash
NO_STRIP=true cargo tauri build
```
