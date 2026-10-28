# xtask

Development tasks for this repository, in the [cargo xtask](https://github.com/matklad/cargo-xtask)
convention: `cargo xtask <task>` (an alias in `.cargo/config.toml`) builds and runs this crate.
Nothing ships it. Git runs as a subprocess.

```sh
# A checkout of a pinned commit or release tag (vX.Y.Z) from <url> in <dir>, kept between runs
# for its build cache; untracked files other than target/ are removed.
cargo xtask dev-cluster checkout <dir> <pin> <url>
```
