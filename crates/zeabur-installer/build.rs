//! Generates the table of embedded web-interface files from `ZEABUR_INSTALLER_UI_DIST`, which
//! `scripts/build-installer-ui.sh` fills ([`zeabur_embed`]).

use proc_macro2::TokenStream;
use quote::quote;
use zeabur_embed::{Entry, Kind, include};

fn main() {
    zeabur_embed::generate(
        "ZEABUR_INSTALLER_UI_DIST",
        "run scripts/build-installer-ui.sh and set ZEABUR_INSTALLER_UI_DIST to its output \
         directory.",
        table,
    );
}

fn table(_: Kind, entries: &[Entry]) -> TokenStream {
    let assets = entries.iter().map(|entry| {
        let Entry {
            path,
            content_type,
            etag,
            immutable,
            file,
            ..
        } = entry;
        let brotli = include(entry.brotli.as_deref());
        quote! {
            Asset {
                path: #path,
                content_type: #content_type,
                etag: #etag,
                immutable: #immutable,
                identity: include_bytes!(#file),
                brotli: #brotli,
            }
        }
    });
    quote! {
        pub(crate) static ASSETS: &[Asset] = &[#(#assets),*];
    }
}
