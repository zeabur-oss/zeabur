//! What this installer release installs, pinned by digest.
//!
//! RKE2 releases carry no signatures or provenance; the only check upstream offers is a
//! `sha256sum-<arch>.txt` served from the same release page, which does not help when that page is
//! the thing tampered with. The digests are therefore copied into the installer, and the
//! installer's own signature is where trust starts. Another RKE2 version can still be chosen, but
//! the plan marks it unsupported and checks it only against the release's list.

/// An RKE2 release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rke2 {
    /// `v1.36.5+rke2r1`.
    pub version: &'static str,
    /// The Kubernetes minor it ships, for the catalog's tested list.
    pub kubernetes: &'static str,
    /// SHA-256 per architecture.
    pub amd64: Rke2Files,
    /// SHA-256 per architecture.
    pub arm64: Rke2Files,
}

/// The files of one architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rke2Files {
    /// `rke2.linux-<arch>.tar.gz`.
    pub tarball: &'static str,
    /// `rke2-images-core.linux-<arch>.tar.zst`.
    pub images_core: &'static str,
    /// `rke2-images-canal.linux-<arch>.tar.zst`.
    pub images_canal: &'static str,
}

/// The RKE2 release this installer installs: the `stable` channel on 2026-10-21.
pub const RKE2: Rke2 = Rke2 {
    version: "v1.36.5+rke2r1",
    kubernetes: "1.36",
    amd64: Rke2Files {
        tarball: "4fdbe8a8a32517fbcb6e80c71ca393b96828fbb87685102d9058b51527f9278d",
        images_core: "adc5fe6ca25a3ebc8866e3dad3ff3690d38c8af4a5d312e71bf954e6e3213203",
        images_canal: "294426213e146205f37037d2ab7bc629225d53a785e3f89d5a5f7405d2f0c98d",
    },
    arm64: Rke2Files {
        tarball: "244454bed11d9525452e10a2e5055da5d5fc6b61928ca3fae778bc4254ce467f",
        images_core: "5030039486973fc679224bf297116df99972cb41ef4dc3387334685c6e1f170f",
        images_canal: "626bb4dc147d51b909aa73fbdaaad4104001ec6a92680be04ed2e974fb3a145c",
    },
};

impl Rke2 {
    /// The pinned files of an architecture (`amd64`, `arm64`).
    #[must_use]
    pub fn files(&self, arch: &str) -> Option<Rke2Files> {
        match arch {
            "amd64" => Some(self.amd64),
            "arm64" => Some(self.arm64),
            _ => None,
        }
    }
}

/// The GitHub release URL of an RKE2 file.
#[must_use]
pub fn rke2_url(version: &str, file: &str) -> String {
    format!(
        "https://github.com/rancher/rke2/releases/download/{}/{file}",
        version.replace('+', "%2B")
    )
}

/// A product release published with a signed checksum list, or `None` while unpublished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Product {
    /// The release tag.
    pub version: &'static str,
    /// Image by digest.
    pub image: Option<&'static str>,
}

/// The catalog release whose `index.json` the installer reads. Unpublished: development installs
/// name an `index.json` in `sources.catalog`.
pub const CATALOG: Option<(&str, &str)> = None;

/// Zeabur Identity. Unpublished: `sources.identityImage` and `sources.identityctl`.
pub const IDENTITY: Product = Product {
    version: "v0.0.1",
    image: None,
};

/// Zeabur. Unpublished: `sources.zeaburImage` and `sources.zeaburChart`.
pub const ZEABUR: Product = Product {
    version: "v0.0.1",
    image: None,
};
