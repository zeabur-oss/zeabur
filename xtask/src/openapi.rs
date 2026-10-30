//! The API's description and the web interface's API client made from it: `zeabur-api`'s test writes
//! `crates/zeabur-api/openapi.yaml` when `ZEABUR_UPDATE_OPENAPI` is set, and `bun run gen:api`
//! turns it into `web/src/api/zeabur.ts`. CI fails when either file is stale.

use std::path::{Path, PathBuf};

use crate::output;
use crate::process::{Cmd, on_path};

/// Rewrites both files from the handlers.
pub(crate) fn generate() -> Result<(), String> {
    if !on_path("bun") {
        return Err("bun is not on PATH; run this inside `nix develop`".to_owned());
    }
    let root = root();
    if !root.join("web/node_modules").is_dir() {
        return Err("web/node_modules is missing; run `bun ci` in web/ first".to_owned());
    }
    Cmd::new("cargo")
        .args([
            "nextest",
            "run",
            "--locked",
            "-p",
            "zeabur-api",
            "committed",
        ])
        .env("ZEABUR_UPDATE_OPENAPI", "1")
        .current_dir(&root)
        .run()?;
    Cmd::new("bun")
        .args(["run", "gen:api"])
        .current_dir(root.join("web"))
        .run()?;
    output::say("updated crates/zeabur-api/openapi.yaml and web/src/api/zeabur.ts");
    Ok(())
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
}
