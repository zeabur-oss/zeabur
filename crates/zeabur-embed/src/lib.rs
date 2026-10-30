#![doc = include_str!("../README.md")]

use std::path::{Component, Path};
use std::{env, fs};

use proc_macro2::{Literal, TokenStream};
use quote::quote;

/// What a build embeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The built interface from the environment variable's directory.
    Dist,
    /// The crate's `placeholder/index.html`.
    Placeholder,
}

/// One file of `manifest.tsv`; `file` and `brotli` are the paths for `include_bytes!`.
#[derive(Debug)]
pub struct Entry {
    /// Path relative to the site root, without a leading `/`.
    pub path: String,
    /// Value of `Content-Type`.
    pub content_type: String,
    /// Strong entity tag of the file, quotes included.
    pub etag: String,
    /// Whether the file name carries a content hash.
    pub immutable: bool,
    /// Length of the file, in bytes.
    pub len: u64,
    /// The file itself.
    pub file: String,
    /// Its brotli encoding, when the manifest lists one.
    pub brotli: Option<String>,
}

/// Writes `OUT_DIR/embedded.rs`: `table` of the entries in `$var`, or of the placeholder in a
/// debug build without `$var`. A release build without it fails with `missing` in the message.
pub fn generate(var: &str, missing: &str, table: impl Fn(Kind, &[Entry]) -> TokenStream) {
    cargo(&format!("rerun-if-env-changed={var}"));
    cargo("rerun-if-changed=placeholder/index.html");
    let code = match env::var_os(var).filter(|v| !v.is_empty()) {
        Some(dist) => match dist_entries(Path::new(&dist)) {
            Ok(entries) => table(Kind::Dist, &entries),
            Err(message) => compile_error(&format!("{var}: {message}")),
        },
        None if env::var("PROFILE").as_deref() == Ok("release") => compile_error(&format!(
            "{var} is not set. A release build must embed the web interface: {missing}"
        )),
        None => match placeholder_entry() {
            Ok(entry) => table(Kind::Placeholder, &[entry]),
            Err(message) => compile_error(&message),
        },
    };
    let out = Path::new(&env::var_os("OUT_DIR").unwrap_or_default()).join("embedded.rs");
    if let Err(error) = fs::write(&out, code.to_string()) {
        cargo(&format!("error=cannot write {}: {error}", out.display()));
    }
}

/// `Some(include_bytes!(file))`, or `None`.
#[must_use]
pub fn include(file: Option<&str>) -> TokenStream {
    file.map_or_else(|| quote!(None), |file| quote!(Some(include_bytes!(#file))))
}

/// `n` as a literal with `_` between groups of three digits, as clippy wants it in the crate
/// that includes the table.
#[must_use]
pub fn grouped(n: u64) -> Literal {
    let digits = n.to_string();
    let mut grouped = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push('_');
        }
        grouped.push(c);
    }
    grouped
        .parse()
        .unwrap_or_else(|_| Literal::u64_unsuffixed(n))
}

#[expect(
    clippy::print_stdout,
    reason = "cargo reads a build script's instructions from its standard output"
)]
fn cargo(instruction: &str) {
    println!("cargo::{instruction}");
}

fn compile_error(message: &str) -> TokenStream {
    quote!(compile_error!(#message);)
}

fn placeholder_entry() -> Result<Entry, String> {
    let index = Path::new(&env::var_os("CARGO_MANIFEST_DIR").unwrap_or_default())
        .join("placeholder/index.html");
    Ok(Entry {
        path: "index.html".to_owned(),
        content_type: "text/html; charset=utf-8".to_owned(),
        etag: "\"placeholder\"".to_owned(),
        immutable: false,
        len: fs::metadata(&index).map_or(0, |m| m.len()),
        file: utf8(&index)?,
        brotli: None,
    })
}

fn dist_entries(dist: &Path) -> Result<Vec<Entry>, String> {
    let manifest = dist.join("manifest.tsv");
    cargo(&format!("rerun-if-changed={}", manifest.display()));
    let text = fs::read_to_string(&manifest)
        .map_err(|e| format!("cannot read {}: {e}", manifest.display()))?;
    let entries = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.is_empty())
        .map(|(number, line)| {
            dist_entry(dist, line).map_err(|e| format!("manifest.tsv line {}: {e}", number + 1))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !entries.iter().any(|entry| entry.path == "index.html") {
        return Err("manifest.tsv has no index.html".to_owned());
    }
    Ok(entries)
}

fn dist_entry(dist: &Path, line: &str) -> Result<Entry, String> {
    let fields: Vec<&str> = line.split('\t').collect();
    let [path, content_type, sha256, cache, encodings] = fields[..] else {
        return Err("expected 5 fields".to_owned());
    };
    let normal = Path::new(path)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if path.is_empty() || !normal || path.contains('\\') {
        return Err(format!("unsafe path {path:?}"));
    }
    if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("bad sha256".to_owned());
    }
    let immutable = match cache {
        "immutable" => true,
        "revalidate" => false,
        other => return Err(format!("cache {other:?}")),
    };
    let listed = |suffix: &str| -> Result<String, String> {
        let full = dist.join(format!("{path}{suffix}"));
        if !full.is_file() {
            return Err(format!("{} is listed but missing", full.display()));
        }
        cargo(&format!("rerun-if-changed={}", full.display()));
        utf8(&full)
    };
    let file = listed("")?;
    let len = fs::metadata(&file)
        .map_err(|e| format!("cannot read {file}: {e}"))?
        .len();
    let brotli = match encodings {
        "br" => Some(listed(".br")?),
        "-" => None,
        other => return Err(format!("encodings {other:?}")),
    };
    Ok(Entry {
        path: path.to_owned(),
        content_type: content_type.to_owned(),
        etag: format!("\"{}\"", sha256.get(..32).unwrap_or(sha256)),
        immutable,
        len,
        file,
        brotli,
    })
}

fn utf8(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{} is not UTF-8", path.display()))
}
