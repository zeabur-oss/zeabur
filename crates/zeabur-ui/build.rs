//! With the `interface` feature, generates the table of Zeabur's web-interface files from
//! `ZEABUR_UI_DIST`, which `scripts/build-ui.sh` fills ([`zeabur_embed`]). Without it, the crate
//! only serves a table another crate embeds, and needs no `ZEABUR_UI_DIST`.

use proc_macro2::TokenStream;
use quote::quote;
use zeabur_embed::{Entry, Kind};

fn main() {
    if std::env::var_os("CARGO_FEATURE_INTERFACE").is_none() {
        return;
    }
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
    let assets = zeabur_embed::assets(entries);
    quote! {
        /// What this binary embeds.
        pub const KIND: EmbeddedKind = EmbeddedKind::#kind;
        #assets
    }
}
