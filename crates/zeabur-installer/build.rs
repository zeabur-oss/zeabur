//! Generates the table of embedded web-interface files from `ZEABUR_INSTALLER_UI_DIST`, which
//! `scripts/build-installer-ui.sh` fills ([`zeabur_embed`]).

fn main() {
    zeabur_embed::generate(
        "ZEABUR_INSTALLER_UI_DIST",
        "run scripts/build-installer-ui.sh and set ZEABUR_INSTALLER_UI_DIST to its output \
         directory.",
        |_, entries| zeabur_embed::assets(entries),
    );
}
