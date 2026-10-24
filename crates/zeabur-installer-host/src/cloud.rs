//! The cloud provider and region of this machine, from its instance metadata.
//!
//! The firmware's DMI strings name the provider, so at most one metadata service is asked, over
//! plain HTTP to the link-local address with short timeouts and no proxy. A machine outside these
//! providers, or with its metadata service turned off, gives `None`; nothing fails.

use std::fs;
use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

use serde_json::Value;
use zeabur_installer_plan::config::Location;

const METADATA: ([u8; 4], u16) = ([169, 254, 169, 254], 80);
const CONNECT: Duration = Duration::from_millis(300);
const READ: Duration = Duration::from_secs(1);

/// Providers whose metadata is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    /// Amazon EC2.
    Aws,
    /// Google Compute Engine.
    Gcp,
    /// Microsoft Azure.
    Azure,
    /// `DigitalOcean`.
    DigitalOcean,
    /// Hetzner Cloud.
    Hetzner,
    /// Vultr.
    Vultr,
    /// Akamai (Linode).
    Linode,
    /// Oracle Cloud.
    Oracle,
}

impl Provider {
    /// The name used in labels and the join configuration.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Aws => "aws",
            Self::Gcp => "gcp",
            Self::Azure => "azure",
            Self::DigitalOcean => "digitalocean",
            Self::Hetzner => "hetzner",
            Self::Vultr => "vultr",
            Self::Linode => "linode",
            Self::Oracle => "oracle",
        }
    }
}

/// The firmware's strings from `/sys/class/dmi/id`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dmi {
    /// `sys_vendor`.
    pub sys_vendor: String,
    /// `product_name`.
    pub product_name: String,
    /// `bios_vendor`.
    pub bios_vendor: String,
    /// `chassis_asset_tag`.
    pub chassis_asset_tag: String,
}

/// Azure's chassis asset tag.
const AZURE_TAG: &str = "7783-7084-3265-9085-8269-3286-77";

/// The provider the firmware names.
#[must_use]
pub fn provider(dmi: &Dmi) -> Option<Provider> {
    let vendor = dmi.sys_vendor.as_str();
    let has = |s: &str, needle: &str| s.to_ascii_lowercase().contains(needle);
    if has(vendor, "amazon") || has(&dmi.bios_vendor, "amazon") {
        Some(Provider::Aws)
    } else if has(&dmi.product_name, "google compute engine") || has(vendor, "google") {
        Some(Provider::Gcp)
    } else if dmi.chassis_asset_tag == AZURE_TAG {
        Some(Provider::Azure)
    } else if has(vendor, "digitalocean") {
        Some(Provider::DigitalOcean)
    } else if has(vendor, "hetzner") {
        Some(Provider::Hetzner)
    } else if has(vendor, "vultr") {
        Some(Provider::Vultr)
    } else if has(vendor, "linode") || has(vendor, "akamai") {
        Some(Provider::Linode)
    } else if dmi.chassis_asset_tag == "OracleCloud.com" {
        Some(Provider::Oracle)
    } else {
        None
    }
}

fn dmi() -> Dmi {
    let read = |name: &str| {
        fs::read_to_string(format!("/sys/class/dmi/id/{name}"))
            .map_or_else(|_| String::new(), |s| s.trim().to_owned())
    };
    Dmi {
        sys_vendor: read("sys_vendor"),
        product_name: read("product_name"),
        bios_vendor: read("bios_vendor"),
        chassis_asset_tag: read("chassis_asset_tag"),
    }
}

/// This machine's provider and region, or `None` when they cannot be read.
#[must_use]
pub fn detect() -> Option<Location> {
    let provider = provider(&dmi())?;
    let found = match provider {
        Provider::Aws => {
            let token = get(
                "PUT",
                "/latest/api/token",
                &[("X-aws-ec2-metadata-token-ttl-seconds", "60")],
            )?;
            let auth = [("X-aws-ec2-metadata-token", token.trim())];
            let region = get("GET", "/latest/meta-data/placement/region", &auth)?;
            let zone = get(
                "GET",
                "/latest/meta-data/placement/availability-zone",
                &auth,
            );
            Some((region, zone))
        }
        Provider::Gcp => gcp_zone(&get(
            "GET",
            "/computeMetadata/v1/instance/zone",
            &[("Metadata-Flavor", "Google")],
        )?),
        Provider::Azure => azure(&get(
            "GET",
            "/metadata/instance/compute?api-version=2021-02-01&format=json",
            &[("Metadata", "true")],
        )?),
        Provider::DigitalOcean => Some((get("GET", "/metadata/v1/region", &[])?, None)),
        Provider::Hetzner => {
            hetzner_zone(&get("GET", "/hetzner/v1/metadata/availability-zone", &[])?)
        }
        Provider::Vultr => vultr(&get("GET", "/v1.json", &[])?),
        Provider::Linode => {
            let token = get(
                "PUT",
                "/v1/token",
                &[("Metadata-Token-Expiry-Seconds", "60")],
            )?;
            linode(&get(
                "GET",
                "/v1/instance",
                &[
                    ("Metadata-Token", token.trim()),
                    ("Accept", "application/json"),
                ],
            )?)
        }
        Provider::Oracle => oracle(&get(
            "GET",
            "/opc/v2/instance/",
            &[("Authorization", "Bearer Oracle")],
        )?),
    };
    let (region, zone) = found?;
    let region = region.trim().to_owned();
    if region.is_empty() {
        return None;
    }
    Some(Location {
        provider: provider.name().to_owned(),
        region,
        zone: zone.map(|z| z.trim().to_owned()).filter(|z| !z.is_empty()),
    })
}

type RegionZone = (String, Option<String>);

/// `projects/<n>/zones/us-central1-a` to the region `us-central1` and the zone.
fn gcp_zone(path: &str) -> Option<RegionZone> {
    let zone = path.trim().rsplit('/').next()?;
    let (region, _) = zone.rsplit_once('-')?;
    Some((region.to_owned(), Some(zone.to_owned())))
}

/// `fsn1-dc14`: the location is the region, the data centre the zone.
fn hetzner_zone(zone: &str) -> Option<RegionZone> {
    let zone = zone.trim();
    let region = zone.split('-').next()?;
    Some((region.to_owned(), Some(zone.to_owned())))
}

fn azure(json: &str) -> Option<RegionZone> {
    let v: Value = serde_json::from_str(json).ok()?;
    let region = v.get("location")?.as_str()?.to_owned();
    let zone = v
        .get("zone")
        .and_then(Value::as_str)
        .filter(|z| !z.is_empty())
        .map(|z| format!("{region}-{z}"));
    Some((region, zone))
}

fn vultr(json: &str) -> Option<RegionZone> {
    let v: Value = serde_json::from_str(json).ok()?;
    let code = v.pointer("/region/regioncode")?.as_str()?;
    Some((code.to_ascii_lowercase(), None))
}

fn linode(json: &str) -> Option<RegionZone> {
    let v: Value = serde_json::from_str(json).ok()?;
    Some((v.get("region")?.as_str()?.to_owned(), None))
}

fn oracle(json: &str) -> Option<RegionZone> {
    let v: Value = serde_json::from_str(json).ok()?;
    let region = v.get("canonicalRegionName")?.as_str()?.to_owned();
    let zone = v
        .get("availabilityDomain")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some((region, zone))
}

/// One HTTP/1.0 request to the metadata address; the body of a 200 response.
fn get(method: &str, path: &str, headers: &[(&str, &str)]) -> Option<String> {
    let addr = SocketAddr::from(METADATA);
    let mut stream = TcpStream::connect_timeout(&addr, CONNECT).ok()?;
    stream.set_read_timeout(Some(READ)).ok()?;
    stream.set_write_timeout(Some(READ)).ok()?;
    let mut request = format!("{method} {path} HTTP/1.0\r\nHost: 169.254.169.254\r\n");
    for (name, value) in headers {
        for part in [*name, ": ", *value, "\r\n"] {
            request.push_str(part);
        }
    }
    request.push_str("Content-Length: 0\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).ok()?;
    let mut response = Vec::new();
    stream.take(64 * 1024).read_to_end(&mut response).ok()?;
    body(&String::from_utf8(response).ok()?)
}

fn body(response: &str) -> Option<String> {
    let (head, body) = response.split_once("\r\n\r\n")?;
    let status = head.lines().next()?.split_whitespace().nth(1)?;
    (status == "200").then(|| body.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dmi(vendor: &str, product: &str, tag: &str) -> Dmi {
        Dmi {
            sys_vendor: vendor.to_owned(),
            product_name: product.to_owned(),
            bios_vendor: String::new(),
            chassis_asset_tag: tag.to_owned(),
        }
    }

    #[test]
    fn firmware_names_the_provider() {
        assert_eq!(
            provider(&dmi("Amazon EC2", "m7g.large", "")),
            Some(Provider::Aws)
        );
        assert_eq!(
            provider(&dmi("Google", "Google Compute Engine", "")),
            Some(Provider::Gcp)
        );
        assert_eq!(
            provider(&dmi("Microsoft Corporation", "Virtual Machine", AZURE_TAG)),
            Some(Provider::Azure)
        );
        // Hyper-V elsewhere is not Azure.
        assert_eq!(
            provider(&dmi("Microsoft Corporation", "Virtual Machine", "")),
            None
        );
        assert_eq!(
            provider(&dmi("Hetzner", "vServer", "")),
            Some(Provider::Hetzner)
        );
        assert_eq!(
            provider(&dmi("DigitalOcean", "Droplet", "")),
            Some(Provider::DigitalOcean)
        );
        assert_eq!(
            provider(&dmi("QEMU", "Standard PC", "OracleCloud.com")),
            Some(Provider::Oracle)
        );
        assert_eq!(
            provider(&dmi("Parallels International GmbH.", "", "")),
            None
        );
    }

    #[test]
    fn metadata_answers() {
        assert_eq!(
            gcp_zone("projects/123/zones/asia-east1-b\n"),
            Some(("asia-east1".to_owned(), Some("asia-east1-b".to_owned())))
        );
        assert_eq!(
            hetzner_zone("fsn1-dc14"),
            Some(("fsn1".to_owned(), Some("fsn1-dc14".to_owned())))
        );
        assert_eq!(
            azure(r#"{"location":"japaneast","zone":"2"}"#),
            Some(("japaneast".to_owned(), Some("japaneast-2".to_owned())))
        );
        assert_eq!(
            azure(r#"{"location":"japaneast","zone":""}"#),
            Some(("japaneast".to_owned(), None))
        );
        assert_eq!(
            vultr(r#"{"region":{"regioncode":"NRT"}}"#),
            Some(("nrt".to_owned(), None))
        );
        assert_eq!(
            linode(r#"{"region":"jp-osa","type":"g6-standard-2"}"#),
            Some(("jp-osa".to_owned(), None))
        );
        assert_eq!(
            oracle(
                r#"{"canonicalRegionName":"ap-tokyo-1","availabilityDomain":"Uocm:AP-TOKYO-1-AD-1"}"#
            ),
            Some((
                "ap-tokyo-1".to_owned(),
                Some("Uocm:AP-TOKYO-1-AD-1".to_owned())
            ))
        );
        assert_eq!(
            body("HTTP/1.0 200 OK\r\nContent-Type: text/plain\r\n\r\nnyc3"),
            Some("nyc3".to_owned())
        );
        assert_eq!(body("HTTP/1.1 401 Unauthorized\r\n\r\n"), None);
    }
}
