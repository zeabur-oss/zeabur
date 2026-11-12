//! Reachability: a TCP connection to the API server's host and port. It says whether the address
//! answers, not whether the credentials work.

use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum Reach {
    Reachable { millis: u64 },
    NoAddress,
    Unresolved { host: String },
    Refused,
    TimedOut,
}

/// `https://host:port/path` to `host:port`, with 443 when the port is missing.
fn host_port(server: &str) -> Option<(String, u16)> {
    let rest = server.split_once("://").map_or(server, |(_, rest)| rest);
    let authority = rest.split('/').next()?;
    if let Some(stripped) = authority.strip_prefix('[') {
        let (host, after) = stripped.split_once(']')?;
        let port = after.strip_prefix(':').and_then(|p| p.parse().ok()).unwrap_or(443);
        return Some((host.to_owned(), port));
    }
    match authority.rsplit_once(':') {
        Some((host, port)) => Some((host.to_owned(), port.parse().ok()?)),
        None => Some((authority.to_owned(), 443)),
    }
}

pub fn probe(server: Option<&str>) -> Reach {
    let Some((host, port)) = server.and_then(host_port) else {
        return Reach::NoAddress;
    };
    let Ok(addrs) = (host.as_str(), port).to_socket_addrs() else {
        return Reach::Unresolved { host };
    };
    let mut refused = false;
    for addr in addrs {
        let started = Instant::now();
        match TcpStream::connect_timeout(&addr, Duration::from_millis(1500)) {
            Ok(_) => {
                let millis = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                return Reach::Reachable { millis };
            }
            Err(err) if err.kind() == std::io::ErrorKind::ConnectionRefused => refused = true,
            Err(_) => {}
        }
    }
    if refused { Reach::Refused } else { Reach::TimedOut }
}

#[cfg(test)]
mod tests {
    use super::host_port;

    #[test]
    fn server_urls_give_host_and_port() {
        assert_eq!(host_port("https://10.0.0.12:6443"), Some(("10.0.0.12".into(), 6443)));
        assert_eq!(host_port("https://api.example.com/k8s"), Some(("api.example.com".into(), 443)));
        assert_eq!(host_port("https://[fd00::1]:6443"), Some(("fd00::1".into(), 6443)));
    }
}
