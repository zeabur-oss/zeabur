//! Generates the table of embedded web-interface files from `ZEABUR_UI_DIST`, which
//! `scripts/build-ui.sh` fills ([`zeabur_embed`]). Files under `assets/` with a brotli encoding
//! keep only that encoding.

use proc_macro2::TokenStream;
use quote::quote;
use zeabur_embed::{Entry, Kind, grouped, include};

fn main() {
    zeabur_embed::generate(
        "ZEABUR_UI_DIST",
        "run scripts/build-ui.sh and set ZEABUR_UI_DIST to its output directory, or build \
         without the `ui` feature (--no-default-features).",
        table,
    );
}

fn table(kind: Kind, entries: &[Entry]) -> TokenStream {
    let kind = match kind {
        Kind::Dist => quote!(Dist),
        Kind::Placeholder => quote!(Placeholder),
    };
    let count = proc_macro2::Literal::usize_unsuffixed(entries.len());
    let assets = entries.iter().map(|entry| {
        let Entry {
            path,
            content_type,
            etag,
            immutable,
            ..
        } = entry;
        let len = grouped(entry.len);
        let brotli_only = entry.brotli.is_some() && path.starts_with("assets/");
        let identity = include((!brotli_only).then_some(entry.file.as_str()));
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
