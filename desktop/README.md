# Zeabur Desktop

A Tauri 2 app that lists the contexts of your kubeconfig files and opens Zeabur OSS on the one you
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
hoc. The version is `desktop/Cargo.toml`'s, which the release sets with the rest of the repository.
Release packages are built outside the Nix shell with `scripts/package-desktop.sh`
([releasing](../docs/internal/releasing.md#zeabur-desktop)).

## Updates

`src/update.rs` and `web/src/chooser/update.tsx`, with Tauri's updater plugin. The app checks the
newest release's `latest.json` once at start, quietly, and when asked (the sidebar's button, on
macOS also **Zeabur > Check for Updates…**); it shows the version and the notes and installs only
after **Update and restart**. What it can do depends on how it was installed: the macOS app, the
NSIS install and the AppImage replace themselves; a `.deb`, `.rpm`, the portable `.exe` and a
development build offer the release page. An update must carry a signature of the key in
`tauri.conf.json` for that exact version; while the key is empty the app does not check.

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
