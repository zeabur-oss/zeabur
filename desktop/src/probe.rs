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

/// `https://host:port/path` to `host:port`. Without a port it is the scheme's default (443 for
/// https, 80 for http); a server without a scheme is taken as https.
fn host_port(server: &str) -> Option<(String, u16)> {
    let url = if server.contains("://") {
        tauri::Url::parse(server)
    } else {
        tauri::Url::parse(&format!("https://{server}"))
    }
    .ok()?;
    // An IPv6 host comes in brackets.
    let host = url
        .host_str()?
        .trim_start_matches('[')
        .trim_end_matches(']');
    Some((host.to_owned(), url.port_or_known_default()?))
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
    if refused {
        Reach::Refused
    } else {
        Reach::TimedOut
    }
}

#[cfg(test)]
mod tests {
    use super::host_port;

    #[test]
    fn server_urls_give_host_and_port() {
        assert_eq!(
            host_port("https://10.0.0.12:6443"),
            Some(("10.0.0.12".into(), 6443))
        );
        assert_eq!(
            host_port("https://api.example.com/k8s"),
            Some(("api.example.com".into(), 443))
        );
        assert_eq!(
            host_port("https://[fd00::1]:6443"),
            Some(("fd00::1".into(), 6443))
        );
        assert_eq!(
            host_port("http://127.0.0.1"),
            Some(("127.0.0.1".into(), 80))
        );
        assert_eq!(
            host_port("10.0.0.12:6443"),
            Some(("10.0.0.12".into(), 6443))
        );
    }
}
