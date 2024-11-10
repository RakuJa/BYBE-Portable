# BYBE-Portable
Multi-platform portable (offline) application that bundles the BYBE website

## Build base package locally for Linux/Windows/MacOS
### Install Tauri-cli
```bash
cargo install tauri-cli
```

### Clone repository
```bash
git clone --recurse-submodules https://github.com/RakuJa/BYBE-Portable.git
```
### Copy the DB in home or change ENV variable
To let BYBE aka BYBE-backend aka BYBE-core compile, you have to let the Rust library find the database.db file.
This can be done either by:
```bash
cp . /full/path/to/BYBE-Portable/BYBE-tauri/data/database.db
```
or by:
```bash
export DATABASE_URL = /full/path/to/BYBE-Portable/BYBE-tauri/data/database.db
```
Important! It's better to avoid the latter, as it could result in unexpected behaviour during dev testing
### Test the application (does not export bundles)
In the project directory, run
```bash
cargo tauri dev
```
### Build the bundle
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
