![App icon](../res/const.svg)
# Constellations

Matrix client built with [libcosmic](https://github.com/pop-os/libcosmic) and [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk).


## Project Status

Alpha quality software. Usable but you should expect bugs, missing features, and potential breaking changes.

The goal is to reach a stable 1.0 release around the same time the underlying [matrix-rust-sdk](https://github.com/matrix-org/matrix-rust-sdk) and [iced-rs](https://iced.rs/) reach their stable releases.

### Recommendations

If you are looking for a more mature Matrix client:

- **[Fractal](https://gitlab.gnome.org/GNOME/fractal)**: A great GTK-based client for the GNOME desktop.
- **[iamb](https://github.com/ulysses-ao/iamb)**: A powerful terminal-based Matrix client for Vim lovers.

## Build

To build Constellations, you will need a new version of Rust using `rustup`. On Debian based distros at least these:
```sh
sudo apt-get update && sudo apt-get install -y pkg-config libxkbcommon-dev libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libglib2.0-dev
```

```bash
cargo build --release
```

To run the application:

```bash
cargo run --release
```
