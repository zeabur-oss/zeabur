# Zeabur Desktop

A Tauri 2 app that lists the contexts of your kubeconfig files and opens Zeabur on the one you
pick. Each cluster window starts its own local service (`zeabur-local`, the one `zeaburctl ui`
runs) and loads it, so the app needs no other program. How to install and use it is in
[docs/install](../docs/install).

`desktop/` is a Cargo workspace of its own with the main workspace's rules
([crates](../docs/internal/crates.md#apps-outside-the-workspace)). CI's `desktop` job lints and
tests it on Linux.

## Run

```sh
nix develop .#desktop -c scripts/build-ui.sh        # web/dist, once
cd desktop && ZEABUR_UI_DIST=$PWD/../web/dist cargo run
```

The app's own pages are in `desktop/web/` (Vite): the chooser and the page a cluster window shows
while its service starts. The Rust side is `desktop/src/`.

## Build

```sh
nix develop .#desktop -c scripts/build-desktop.sh
```

On macOS this makes `Zeabur.app` and a `.dmg` under `target/desktop/release/bundle/`. Signing and
notarization follow the Tauri bundler's `APPLE_*` variables; without them the app is signed ad
hoc. Releases are built by the release workflow ([releasing](../docs/internal/releasing.md)).

## Screenshots

The images of the chooser and the cluster error page are an image set, `desktop`, taken with
Chromium from the pages of `desktop/web` with mocked commands (`docs/internal/screenshots.md`):

```sh
nix develop -c scripts/docs-shots.sh desktop
```

## Build for Windows

From macOS or Linux with [cargo-xwin](https://github.com/rust-cross/cargo-xwin):

```sh
cd desktop
cargo xwin build --release --target aarch64-pc-windows-msvc   # or x86_64-pc-windows-msvc
```

CI does not build for Windows or macOS. After a change to their code, run `cargo lint` on a Mac
and `cargo xwin clippy --all-targets --all-features --locked --target <target> -- -D warnings`.
