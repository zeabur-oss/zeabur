# zeabur-ui

The web interface, embedded in the binary, and the handler that serves it.

## Place in the workspace

Level 1: no workspace dependency. The `ui` feature of the binaries pulls it in; building with
`--no-default-features` leaves it out and needs no JavaScript toolchain.

## Embedding

The interface (React, in `web/`) is built before Cargo runs, and its output is not committed
(the approach Tauri takes [tauri-codegen]):

1. `scripts/build-ui.sh` runs Vite, then `web/scripts/postbuild.ts` writes a brotli encoding
   (quality 11) of every compressible file and `manifest.tsv`: path, MIME type, SHA-256, cache
   class and whether a brotli encoding exists. It fails if `index.html` carries an inline script or style.
2. `build.rs` reads only the directory named by `ZEABUR_UI_DIST` and generates a table of
   `include_bytes!` entries from the manifest. It runs no JavaScript tool and makes no network
   request. Without `ZEABUR_UI_DIST`, a debug build embeds a one-page placeholder and a release
   build stops with `compile_error!`, so a release binary cannot ship without its interface by
   accident.

Unlike Tauri, which stores brotli only and decompresses on every request [tauri-utils], the table
keeps each file with its precompressed brotli encoding: the server never compresses or decompresses at
run time, at the cost of a larger binary.

## Serving

[`respond`] answers every request that is not an API route:

- **Caching.** Files under `assets/` carry a content hash in their name and are sent with
  `Cache-Control: public, max-age=31536000, immutable`, so browsers do not revalidate them
  [rfc8246]. Everything else, `index.html` in particular, is `no-cache` with a strong entity tag;
  a matching `If-None-Match` (weak comparison) gives `304 Not Modified` [rfc9110] (§8.8.3,
  §13.1.2). Each content coding is a separate representation with its own tag.
- **Encoding.** Brotli when `Accept-Encoding` allows it, otherwise the file itself; `Vary:
  Accept-Encoding` on files that have an encoding. Only brotli is stored: browsers send `br`
  over HTTPS and to `localhost`, and a client without it gets the uncompressed file, which costs
  nothing on loopback. Precompressing at the highest quality beats on-the-fly compression by a
  proxy (and local mode has none); zstd at level 19 is about 5% larger than brotli 11 for the
  same JavaScript bundle and saves nothing for files compressed ahead of time.
- **Single-page fallback.** An unknown path is a client-side route and gets `index.html`. Two
  exceptions return `404` as RFC 9457 Problem Details: paths under `/api` (an unknown API route
  is never a page) and missing files under `assets/` (a stale script reference should fail
  loudly, not receive HTML).
- **Content Security Policy.** [`CSP`]: everything from this origin, no inline script, no
  framing, Trusted Types with no policy. Inline styles are allowed, as Sonner and CodeMirror add
  `<style>` elements at run time (see [`CSP`]). Vite's module-preload polyfill is turned off
  (`build.modulePreload.polyfill: false`); it is bundled into the entry script rather than
  inlined [vite-build], but every supported browser implements `modulepreload`, so it would only
  add code and a `fetch`.

## References

- [tauri-codegen] The Tauri Programme, `embedded_assets.rs`, tauri-codegen, tag tauri-v2.12.3.
  <https://github.com/tauri-apps/tauri/blob/tauri-v2.12.3/crates/tauri-codegen/src/embedded_assets.rs>
- [tauri-utils] The Tauri Programme, `assets.rs`, tauri-utils, tag tauri-v2.12.3.
  <https://github.com/tauri-apps/tauri/blob/tauri-v2.12.3/crates/tauri-utils/src/assets.rs>
- [rfc8246] P. McManus, "HTTP Immutable Responses," RFC 8246, IETF, Sept. 2017.
  <https://www.rfc-editor.org/rfc/rfc8246>
- [rfc9110] R. Fielding, M. Nottingham and J. Reschke, Eds., "HTTP Semantics," RFC 9110,
  IETF, June 2022. <https://www.rfc-editor.org/rfc/rfc9110>
- [rfc9457] M. Nottingham, E. Wilde and S. Dalal, "Problem Details for HTTP APIs," RFC 9457,
  IETF, July 2023. <https://www.rfc-editor.org/rfc/rfc9457>
- [vite-build] Vite, "Build Options: build.modulePreload." <https://vite.dev/config/build-options#build-modulepreload>

[tauri-codegen]: https://github.com/tauri-apps/tauri/blob/tauri-v2.12.3/crates/tauri-codegen/src/embedded_assets.rs
[tauri-utils]: https://github.com/tauri-apps/tauri/blob/tauri-v2.12.3/crates/tauri-utils/src/assets.rs
[rfc8246]: https://www.rfc-editor.org/rfc/rfc8246
[rfc9110]: https://www.rfc-editor.org/rfc/rfc9110
[rfc9457]: https://www.rfc-editor.org/rfc/rfc9457
[vite-build]: https://vite.dev/config/build-options#build-modulepreload
