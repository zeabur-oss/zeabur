//! The node's name in Kubernetes.
//!
//! K3s, RKE2 and `KubeSolo` name a node after the host name unless told otherwise. A host name
//! that changes, as some hypervisors and clouds do on reboot, registers a second node with the
//! same address: the first goes `NotReady` and Calico refuses to start on the address conflict.
//! The installer picks the name once and writes it into the distribution's configuration.

use crate::PlanError;
use crate::plan::Facts;

/// A Kubernetes node name: a lowercase DNS-1123 subdomain.
#[must_use]
pub fn is_valid(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 253
        && name.split('.').all(|label| {
            !label.is_empty()
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

/// Checks `host.nodeName`.
///
/// # Errors
///
/// When it is not a Kubernetes node name.
pub fn check(name: &str) -> Result<(), PlanError> {
    if is_valid(name) {
        Ok(())
    } else {
        Err(PlanError::Config(format!(
            "host.nodeName `{name}` is not a Kubernetes node name: use lowercase letters, digits, \
             `-` and `.`, starting and ending with a letter or digit"
        )))
    }
}

/// The host name in lower case, when it is a valid name and not made from an address the way
/// clouds name hosts (`ip-10-0-0-5`, `10-211-55-189`).
#[must_use]
pub fn from_hostname(hostname: &str, addresses: &[String]) -> Option<String> {
    let name = hostname.trim().to_ascii_lowercase();
    let made_from_address = addresses
        .iter()
        .filter(|a| !a.contains(':'))
        .any(|a| name.contains(a.as_str()) || name.contains(&a.replace('.', "-")))
        || has_ipv4(&name);
    (is_valid(&name) && !made_from_address).then_some(name)
}

/// Four numbers of at most 255 in a row, joined by `-` or `.`.
fn has_ipv4(name: &str) -> bool {
    let mut run = 0;
    for part in name.split(['-', '.']) {
        let octet = !part.is_empty()
            && part.len() <= 3
            && part.bytes().all(|b| b.is_ascii_digit())
            && part.parse::<u16>().is_ok_and(|n| n <= 255);
        run = if octet { run + 1 } else { 0 };
        if run == 4 {
            return true;
        }
    }
    false
}

/// The node's name: the one it already has, else the configured one, else the host name when
/// [`from_hostname`] takes it and no other node of the cluster (`taken`) has it, else
/// `node-<6 hex>`.
///
/// `-2`, `-3`… follow the generated name when another node has it.
///
/// # Errors
///
/// When the configuration renames a node that already has a name, or names one another node
/// has.
pub fn choose(
    configured: Option<&str>,
    facts: &Facts,
    taken: &[String],
) -> Result<String, PlanError> {
    if let Some(existing) = &facts.node_name {
        return match configured {
            Some(name) if name != existing => Err(PlanError::Config(format!(
                "host.nodeName: this host is already node `{existing}`; another name would add a \
                 second node with the same address. Set host.nodeName: {existing} or remove it"
            ))),
            _ => Ok(existing.clone()),
        };
    }
    if let Some(name) = configured {
        check(name)?;
        if taken.iter().any(|t| t == name) {
            return Err(PlanError::Config(format!(
                "host.nodeName: the cluster already has a node `{name}`; choose another name"
            )));
        }
        return Ok(name.to_owned());
    }
    if let Some(name) =
        from_hostname(&facts.hostname, &facts.node_ips).filter(|name| !taken.contains(name))
    {
        return Ok(name);
    }
    let base = format!("node-{}", facts.host_hex);
    let mut name = base.clone();
    for n in 2.. {
        if !taken.contains(&name) {
            break;
        }
        name = format!("{base}-{n}");
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(hostname: &str, existing: Option<&str>) -> Facts {
        Facts {
            arch: "amd64".to_owned(),
            hostname: hostname.to_owned(),
            node_name: existing.map(str::to_owned),
            host_hex: "a1b2c3".to_owned(),
            memory_mib: 4096,
            node_ips: vec!["10.211.55.189".to_owned(), "fd00::5".to_owned()],
            record: None,
            installer: None,
        }
    }

    #[test]
    fn a_normal_host_name_becomes_the_name_in_lower_case() {
        assert_eq!(choose(None, &facts("VPS-1", None), &[]).unwrap(), "vps-1");
        assert_eq!(
            choose(None, &facts("web.example.com", None), &[]).unwrap(),
            "web.example.com"
        );
    }

    #[test]
    fn a_host_name_made_from_an_address_gets_a_generated_name() {
        for hostname in [
            "10-211-55-189",
            "ip-10-211-55-189",
            "ip-10-211-55-189.ec2.internal",
            "10.211.55.189",
            "ip-172-31-4-20",
            "vps_1",
            "",
        ] {
            assert_eq!(
                choose(None, &facts(hostname, None), &[]).unwrap(),
                "node-a1b2c3",
                "{hostname}"
            );
        }
        assert_eq!(
            choose(None, &facts("k8s-1-2", None), &[]).unwrap(),
            "k8s-1-2"
        );
    }

    #[test]
    fn a_configured_name_wins_over_the_host_name() {
        assert_eq!(
            choose(Some("db-1"), &facts("vps-1", None), &[]).unwrap(),
            "db-1"
        );
    }

    #[test]
    fn invalid_names_are_rejected() {
        for bad in ["Vps-1", "-a", "a-", "a..b", "a_b", "", &"a".repeat(254)] {
            assert!(check(bad).is_err(), "{bad}");
            assert!(
                choose(Some(bad), &facts("vps-1", None), &[]).is_err(),
                "{bad}"
            );
        }
        assert!(check("a.b-c.0").is_ok());
    }

    #[test]
    fn a_rerun_keeps_the_name_the_node_already_has() {
        let renamed = facts("parallels-vm", Some("vps-1"));
        assert_eq!(choose(None, &renamed, &[]).unwrap(), "vps-1");
        assert_eq!(choose(Some("vps-1"), &renamed, &[]).unwrap(), "vps-1");
        assert!(choose(Some("vps-2"), &renamed, &[]).is_err());
    }

    #[test]
    fn a_joining_node_takes_a_name_no_other_node_has() {
        let taken = vec!["vps-1".to_owned()];
        assert_eq!(
            choose(None, &facts("vps-1", None), &taken).unwrap(),
            "node-a1b2c3"
        );
        assert!(choose(Some("vps-1"), &facts("vps-2", None), &taken).is_err());
        let taken = ["vps-1", "node-a1b2c3", "node-a1b2c3-2"].map(str::to_owned);
        assert_eq!(
            choose(None, &facts("vps-1", None), &taken).unwrap(),
            "node-a1b2c3-3"
        );
    }
}
