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

/// The releases that a release build pins.
///
/// `scripts/release-pins.sh` reads their signed checksum lists, and the release workflow passes
/// the digests to the compiler. A development build pins none of them, so its configuration names
/// each one in `sources`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Products {
    /// The catalog release whose `index.json` the installer reads.
    pub catalog: Catalog,
    /// Zeabur Identity.
    pub identity: Identity,
    /// Zeabur, of this installer's own release.
    pub zeabur: Zeabur,
}

/// A catalog release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Catalog {
    /// The release tag.
    pub version: &'static str,
    /// SHA-256 of its `index.json`.
    pub index_sha256: &'static str,
}

/// A Zeabur Identity release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Identity {
    /// The release tag.
    pub version: &'static str,
    /// The image index by digest.
    pub image: &'static str,
    /// SHA-256 of `zeabur-identityctl-<version>-linux-amd64.tar.gz`.
    pub identityctl_amd64: &'static str,
    /// SHA-256 of `zeabur-identityctl-<version>-linux-arm64.tar.gz`.
    pub identityctl_arm64: &'static str,
}

/// The Zeabur release of this installer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Zeabur {
    /// The release tag.
    pub version: &'static str,
    /// The image index by digest.
    pub image: &'static str,
    /// SHA-256 of the packaged chart, `zeabur-<version>.tgz`.
    pub chart_sha256: &'static str,
}

/// A file of a GitHub release and its SHA-256.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// The file name.
    pub file: String,
    /// The download URL.
    pub url: String,
    /// The expected SHA-256.
    pub sha256: &'static str,
}

fn asset(repository: &str, version: &str, file: String, sha256: &'static str) -> Asset {
    Asset {
        url: format!(
            "https://github.com/zeabur-oss/{repository}/releases/download/{version}/{file}"
        ),
        file,
        sha256,
    }
}

impl Catalog {
    /// The release's `index.json`.
    #[must_use]
    pub fn index(&self) -> Asset {
        asset(
            "catalog",
            self.version,
            "index.json".to_owned(),
            self.index_sha256,
        )
    }
}

impl Identity {
    /// The `zeabur-identityctl` archive of an architecture (`amd64`, `arm64`).
    #[must_use]
    pub fn identityctl(&self, arch: &str) -> Option<Asset> {
        let sha256 = match arch {
            "amd64" => self.identityctl_amd64,
            "arm64" => self.identityctl_arm64,
            _ => return None,
        };
        let file = format!("zeabur-identityctl-{}-linux-{arch}.tar.gz", self.version);
        Some(asset("identity", self.version, file, sha256))
    }
}

impl Zeabur {
    /// The packaged chart.
    #[must_use]
    pub fn chart(&self) -> Asset {
        let file = format!("zeabur-{}.tgz", self.version.trim_start_matches('v'));
        asset("zeabur", self.version, file, self.chart_sha256)
    }
}

/// The pinned releases, or `None` in a development build.
pub const PRODUCTS: Option<Products> = match (
    option_env!("ZEABUR_PIN_CATALOG_VERSION"),
    option_env!("ZEABUR_PIN_CATALOG_SHA256"),
    option_env!("ZEABUR_PIN_IDENTITY_VERSION"),
    option_env!("ZEABUR_PIN_IDENTITY_IMAGE"),
    option_env!("ZEABUR_PIN_IDENTITYCTL_AMD64_SHA256"),
    option_env!("ZEABUR_PIN_IDENTITYCTL_ARM64_SHA256"),
    option_env!("ZEABUR_PIN_ZEABUR_IMAGE"),
    option_env!("ZEABUR_PIN_ZEABUR_CHART_SHA256"),
) {
    (
        Some(catalog_version),
        Some(index_sha256),
        Some(identity_version),
        Some(identity_image),
        Some(identityctl_amd64),
        Some(identityctl_arm64),
        Some(zeabur_image),
        Some(chart_sha256),
    ) => Some(Products {
        catalog: Catalog {
            version: catalog_version,
            index_sha256,
        },
        identity: Identity {
            version: identity_version,
            image: identity_image,
            identityctl_amd64,
            identityctl_arm64,
        },
        zeabur: Zeabur {
            version: concat!("v", env!("CARGO_PKG_VERSION")),
            image: zeabur_image,
            chart_sha256,
        },
    }),
    _ => None,
};

/// A file of one architecture and its SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchFiles {
    /// SHA-256 of the amd64 file.
    pub amd64: &'static str,
    /// SHA-256 of the arm64 file.
    pub arm64: &'static str,
}

impl ArchFiles {
    /// The SHA-256 of an architecture (`amd64`, `arm64`).
    #[must_use]
    pub const fn get(&self, arch: &str) -> Option<&'static str> {
        match arch.as_bytes() {
            b"amd64" => Some(self.amd64),
            b"arm64" => Some(self.arm64),
            _ => None,
        }
    }
}

/// A `KubeSolo` release, for a personal install.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KubeSolo {
    /// `v1.2.2`.
    pub version: &'static str,
    /// The Kubernetes minor it ships, for the catalog's tested list.
    pub kubernetes: &'static str,
    /// The offline archive, which carries the images `KubeSolo` runs (pause, `CoreDNS`,
    /// local-path-provisioner). The online one pulls them by tag.
    pub offline: ArchFiles,
}

/// The `KubeSolo` release a personal install runs. Upstream publishes checksums only on the
/// release page, so the digests are copied here as for RKE2.
pub const KUBESOLO: KubeSolo = KubeSolo {
    version: "v1.2.2",
    kubernetes: "1.35",
    offline: ArchFiles {
        amd64: "44e434ed77438556cb6cb2ee255339de3af8473109a93fa64df7b614e041218b",
        arm64: "85ab7edb265b1f1559a1937b92eee8d6b0352c3c7fe78f4c641752b7cacc3a5c",
    },
};

impl KubeSolo {
    /// The offline archive of an architecture.
    #[must_use]
    pub fn archive(&self, arch: &str) -> Option<Asset> {
        let sha256 = self.offline.get(arch)?;
        let file = format!("kubesolo-{}-linux-{arch}-offline.tar.gz", self.version);
        Some(Asset {
            url: format!(
                "https://github.com/portainer/kubesolo/releases/download/{}/{file}",
                self.version
            ),
            file,
            sha256,
        })
    }
}

/// The standalone helm-controller, which installs `HelmChart` objects on a cluster without one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HelmController {
    /// `v0.17.9`.
    pub version: &'static str,
    /// SHA-256 of `deploy-cluster-scoped.yaml`.
    pub manifest_sha256: &'static str,
    /// The images the manifest names by tag, and the digest each one is pinned to.
    pub images: &'static [(&'static str, &'static str)],
}

/// The helm-controller release the catalog tests on `KubeSolo` (catalog `config/kubernetes.yaml`).
pub const HELM_CONTROLLER: HelmController = HelmController {
    version: "v0.17.9",
    manifest_sha256: "53da110ca271b6d6415e84bfc5ffe47d726fa35d1fb3acda47e6649ee05d4791",
    images: &[
        (
            "rancher/helm-controller:v0.17.9",
            "sha256:25a4fcbbf9fa8396ded332165e4497eb45e08a8f6677158c0f861a2d15982afc",
        ),
        (
            "ghcr.io/k3s-io/klipper-helm:v0.9.10-build20251111",
            "sha256:251a6a3983934f1026c34f4337fc5a87e093a142438c7d16f7b31c179162176e",
        ),
    ],
};

impl HelmController {
    /// The manifest.
    #[must_use]
    pub fn manifest(&self) -> Asset {
        let file = "deploy-cluster-scoped.yaml".to_owned();
        Asset {
            url: format!(
                "https://github.com/k3s-io/helm-controller/releases/download/{}/{file}",
                self.version
            ),
            file,
            sha256: self.manifest_sha256,
        }
    }
}

/// kubectl of the Kubernetes release `KubeSolo` ships; `KubeSolo` has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Kubectl {
    /// `v1.35.7`.
    pub version: &'static str,
    /// SHA-256 of the binary.
    pub binary: ArchFiles,
}

/// The kubectl a personal install uses.
pub const KUBECTL: Kubectl = Kubectl {
    version: "v1.35.7",
    binary: ArchFiles {
        amd64: "12e97f9d23a9f6cbb87b89becd6bd291e1a858a3379a4e11e2c822c4c1530052",
        arm64: "5103d45b8881434d417694057b8ccb5ae79fd310a5c5c13e403c5e62e15909be",
    },
};

impl Kubectl {
    /// The binary of an architecture.
    #[must_use]
    pub fn binary(&self, arch: &str) -> Option<Asset> {
        let sha256 = self.binary.get(arch)?;
        Some(Asset {
            url: format!(
                "https://dl.k8s.io/release/{}/bin/linux/{arch}/kubectl",
                self.version
            ),
            file: "kubectl".to_owned(),
            sha256,
        })
    }
}
