# BYBE-Portable
Multi-platform portable (offline) application that bundles the BYBE website

## Build base package locally for Linux/Windows/MacOS
This build process takes care of everything. You must be connected to the internet during the build process to download required files.
### Clone repository
```bash
git clone --recurse-submodules https://github.com/RakuJa/BYBE-Portable.git
```

### Install Tauri-cli
```bash
cargo install tauri-cli
```

### Install cargo make
```bash
cargo install --force cargo-make
```

### Run the build
```bash
cargo make build-app
```

### Fetch the resulting exe/dmg/appimage
You should find the result of the build process inside BYBE-tauri/target/release/bundle

## Test and build package while developing 
Building and testing the package while developing is very slow if you are using the clean build process used for releasing the package.
To reach faster compile time and adding the possibility of debugging we may use the following strategy.

### Download the DB
Fetch the correct DB release (should be the highest release number that is <= BYBE-BE version)

ex:

BYBE-BE version = 3.4.0

BYBE-DB versions = 2.3.0, 2.4.0, 3.0.0, 3.5.0

The highest version that is <= 3.4.0 is 3.0.0

or automatically download using
```bash
cargo make prebuild
```

### Test the application (does not export bundles)
In the project directory, run
```bash
cargo tauri dev
```
