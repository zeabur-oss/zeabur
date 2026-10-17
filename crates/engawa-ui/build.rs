//! Generates the table of embedded web-interface files.
//!
//! The only input is the directory named by `ENGAWA_UI_DIST`, which `scripts/build-ui.sh` fills
//! with the built interface, its brotli encodings and `manifest.tsv`. This script runs no
//! JavaScript tool and makes no network request, so `cargo build` stays hermetic.
//!
//! Without `ENGAWA_UI_DIST`, a debug build embeds a one-page placeholder and a release build stops
//! with a compile error: a release binary without its interface would be a silent mistake.

use std::fmt::Write as _;
use std::path::{Component, Path, PathBuf};
use std::{env, fs};

const VAR: &str = "ENGAWA_UI_DIST";

fn main() {
    println!("cargo::rerun-if-env-changed={VAR}");
    println!("cargo::rerun-if-changed=placeholder/index.html");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap_or_default()).join("embedded.rs");
    let code = match env::var_os(VAR).filter(|v| !v.is_empty()) {
        Some(dist) => match dist_table(Path::new(&dist)) {
            Ok(code) => code,
            Err(message) => compile_error(&format!("{VAR}: {message}")),
        },
        None if env::var("PROFILE").as_deref() == Ok("release") => compile_error(&format!(
            "{VAR} is not set. A release build must embed the web interface: run \
             scripts/build-ui.sh and set {VAR} to its output directory, or build without the \
             `ui` feature (--no-default-features)."
        )),
        None => placeholder_table(),
    };
    if let Err(error) = fs::write(&out, code) {
        println!("cargo::error=cannot write {}: {error}", out.display());
    }
}

fn compile_error(message: &str) -> String {
    format!("compile_error!({message:?});\n")
}

fn placeholder_table() -> String {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default());
    let index = manifest_dir.join("placeholder/index.html");
    format!(
        "/// What this binary embeds.\npub const KIND: EmbeddedKind = EmbeddedKind::Placeholder;\n\
         pub(crate) static ASSETS: &[Asset] = &[Asset {{ path: \"index.html\", \
         content_type: \"text/html; charset=utf-8\", etag: \"\\\"placeholder\\\"\", \
         immutable: false, identity: include_bytes!({:?}), brotli: None }}];\n",
        index.display().to_string()
    )
}

/// One line of `manifest.tsv`: path, MIME type, SHA-256 (hex) of the file, cache class
/// (`immutable` or `revalidate`), encodings (`br` or `-`).
fn dist_table(dist: &Path) -> Result<String, String> {
    let manifest = dist.join("manifest.tsv");
    println!("cargo::rerun-if-changed={}", manifest.display());
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let mut code = String::from(
        "/// What this binary embeds.\npub const KIND: EmbeddedKind = EmbeddedKind::Dist;\n\
         pub(crate) static ASSETS: &[Asset] = &[\n",
    );
    let mut has_index = false;
    for (number, line) in text.lines().enumerate().filter(|(_, l)| !l.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [path, content_type, sha256, cache, encodings] = fields[..] else {
            return Err(format!(
                "manifest.tsv line {}: expected 5 fields",
                number + 1
            ));
        };
        let relative = Path::new(path);
        if path.is_empty()
            || !relative
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
            || path.contains('\\')
        {
            return Err(format!(
                "manifest.tsv line {}: unsafe path {path:?}",
                number + 1
            ));
        }
        if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(format!("manifest.tsv line {}: bad sha256", number + 1));
        }
        let immutable = match cache {
            "immutable" => true,
            "revalidate" => false,
            other => return Err(format!("manifest.tsv line {}: cache {other:?}", number + 1)),
        };
        has_index |= path == "index.html";
        let short = sha256.get(..32).unwrap_or(sha256);
        let file = |suffix: &str| -> Result<String, String> {
            let full = dist.join(format!("{path}{suffix}"));
            if !full.is_file() {
                return Err(format!("{} is listed but missing", full.display()));
            }
            println!("cargo::rerun-if-changed={}", full.display());
            Ok(format!("include_bytes!({:?})", full.display().to_string()))
        };
        let brotli = match encodings {
            "br" => format!("Some({})", file(".br")?),
            "-" => "None".to_owned(),
            other => {
                return Err(format!(
                    "manifest.tsv line {}: encodings {other:?}",
                    number + 1
                ));
            }
        };
        writeln!(
            code,
            "    Asset {{ path: {path:?}, content_type: {content_type:?}, etag: {etag:?}, \
             immutable: {immutable}, identity: {identity}, brotli: {brotli} }},",
            etag = format!("\"{short}\""),
            identity = file("")?,
        )
        .map_err(|e| e.to_string())?;
    }
    if !has_index {
        return Err("manifest.tsv has no index.html".to_owned());
    }
    code.push_str("];\n");
    Ok(code)
}
