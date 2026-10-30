//! Generates the table of embedded web-interface files.
//!
//! The only input is the directory named by `ZEABUR_UI_DIST`, which `scripts/build-ui.sh` fills
//! with the built interface, its brotli encodings and `manifest.tsv`. This script runs no
//! JavaScript tool and makes no network request, so `cargo build` stays hermetic.
//!
//! Without `ZEABUR_UI_DIST`, a debug build embeds a one-page placeholder and a release build stops
//! with a compile error: a release binary without its interface would be a silent mistake.

use std::path::{Component, Path};
use std::{env, fs};

use proc_macro2::{Literal, TokenStream};
use quote::quote;

const VAR: &str = "ZEABUR_UI_DIST";

/// One embedded file; `identity` and `brotli` are the paths `include_bytes!` reads.
struct Entry {
    path: String,
    content_type: String,
    etag: String,
    immutable: bool,
    len: u64,
    identity: Option<String>,
    brotli: Option<String>,
}

fn main() {
    println!("cargo::rerun-if-env-changed={VAR}");
    println!("cargo::rerun-if-changed=placeholder/index.html");
    let code = match env::var_os(VAR).filter(|v| !v.is_empty()) {
        Some(dist) => match dist_entries(Path::new(&dist)) {
            Ok(entries) => table(&quote!(Dist), &entries),
            Err(message) => compile_error(&format!("{VAR}: {message}")),
        },
        None if env::var("PROFILE").as_deref() == Ok("release") => compile_error(&format!(
            "{VAR} is not set. A release build must embed the web interface: run \
             scripts/build-ui.sh and set {VAR} to its output directory, or build without the \
             `ui` feature (--no-default-features)."
        )),
        None => match placeholder_entry() {
            Ok(entry) => table(&quote!(Placeholder), &[entry]),
            Err(message) => compile_error(&message),
        },
    };
    let out = Path::new(&env::var_os("OUT_DIR").unwrap_or_default()).join("embedded.rs");
    if let Err(error) = fs::write(&out, code.to_string()) {
        println!("cargo::error=cannot write {}: {error}", out.display());
    }
}

fn table(kind: &TokenStream, entries: &[Entry]) -> TokenStream {
    let count = Literal::usize_unsuffixed(entries.len());
    let assets = entries.iter().map(|entry| {
        let Entry {
            path,
            content_type,
            etag,
            immutable,
            len,
            ..
        } = entry;
        let len = grouped(*len);
        let identity = include(entry.identity.as_deref());
        let brotli = include(entry.brotli.as_deref());
        quote! {
            Asset {
                path: #path,
                content_type: #content_type,
                etag: #etag,
                immutable: #immutable,
                len: #len,
                identity: #identity,
                brotli: #brotli,
                decoded: Decoded::new(),
            }
        }
    });
    quote! {
        /// What this binary embeds.
        pub const KIND: EmbeddedKind = EmbeddedKind::#kind;
        pub(crate) static ASSETS: [Asset; #count] = [#(#assets),*];
    }
}

/// `len` as a literal with `_` between groups of three digits, as clippy wants it.
fn grouped(n: u64) -> Literal {
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

fn include(file: Option<&str>) -> TokenStream {
    file.map_or_else(|| quote!(None), |file| quote!(Some(include_bytes!(#file))))
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
        identity: Some(utf8(&index)?),
        brotli: None,
    })
}

/// Each line of `manifest.tsv`: path, MIME type, SHA-256 (hex) of the file, cache class
/// (`immutable` or `revalidate`), encodings (`br` or `-`).
fn dist_entries(dist: &Path) -> Result<Vec<Entry>, String> {
    let manifest = dist.join("manifest.tsv");
    println!("cargo::rerun-if-changed={}", manifest.display());
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
    let file = |suffix: &str| -> Result<String, String> {
        let full = dist.join(format!("{path}{suffix}"));
        if !full.is_file() {
            return Err(format!("{} is listed but missing", full.display()));
        }
        println!("cargo::rerun-if-changed={}", full.display());
        utf8(&full)
    };
    let original = file("")?;
    let len = fs::metadata(&original)
        .map_err(|e| format!("cannot read {original}: {e}"))?
        .len();
    let (identity, brotli) = match encodings {
        "br" if path.starts_with("assets/") => (None, Some(file(".br")?)),
        "br" => (Some(original), Some(file(".br")?)),
        "-" => (Some(original), None),
        other => return Err(format!("encodings {other:?}")),
    };
    Ok(Entry {
        path: path.to_owned(),
        content_type: content_type.to_owned(),
        etag: format!("\"{}\"", sha256.get(..32).unwrap_or(sha256)),
        immutable,
        len,
        identity,
        brotli,
    })
}

fn utf8(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("{} is not UTF-8", path.display()))
}
