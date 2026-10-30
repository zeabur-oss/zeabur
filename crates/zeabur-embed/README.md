# zeabur-embed

The build-script side of embedding a built web interface, shared by `zeabur-ui` and
`zeabur-installer`. Only ever a build-dependency: nothing here reaches a binary.

[`generate`] reads the directory named by an environment variable, which a `scripts/build-*ui.sh`
fills with the built interface, its brotli encodings and `manifest.tsv`, and writes the table
the crate's own `table` function makes from its [`Entry`]s to `OUT_DIR/embedded.rs`. It runs no
JavaScript tool and makes no network request, so `cargo build` stays hermetic.

Without the variable, a debug build embeds the crate's `placeholder/index.html` and a release
build stops with a compile error: a release binary without its interface would be a silent
mistake.

Each line of `manifest.tsv` is a path, a MIME type, the SHA-256 (hex) of the file, a cache class
(`immutable` or `revalidate`) and the encodings (`br` or `-`), separated by tabs.
