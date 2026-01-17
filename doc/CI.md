# Continuous Integration

Gradiance uses a self-hosted CI runner to handle the specific requirements of the Bevy engine and its Linux dependencies (Alsa, Udev, Wayland, X11).

## Runner Configuration

The CI pipeline runs on a self-hosted machine tagged `[self-hosted, linux-desktop]`. This machine must have the required system dependencies installed.

### Dependencies

The build environment requires several system libraries, which can be installed using the `setup_env.sh` script in the repository root:

```bash
# setup_env.sh contents:
export PKG_CONFIG_PATH=/usr/lib/x86_64-linux-gnu/pkgconfig:$PKG_CONFIG_PATH
sudo apt-get update
sudo apt-get install -y g++ pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
```

**Note:** The `PKG_CONFIG_PATH` export is crucial for finding the correct library versions in some containerized environments.

## Workflows

### Build and Test (`.github/workflows/build-and-test.yml`)

This workflow runs on every push to `main` and on pull requests. It performs the following checks:

1.  **Format**: `cargo fmt --all -- --check`
2.  **Lint**: `cargo clippy --all-targets --all-features -- -D warnings`
3.  **Test**: `cargo test`

### Documentation Sync (`.github/workflows/docs-sync.yml`)

Syncs documentation assets.
