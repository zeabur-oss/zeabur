//! Reaching another node: TCP connect times and open ports.
//!
//! A TCP connect takes one round trip (SYN, SYN-ACK), so its duration is a fair measure of the
//! distance to the server without ICMP, which clouds often block. UDP has no handshake: a UDP port
//! cannot be shown open from one side.

use std::net::{SocketAddr, TcpStream, ToSocketAddrs as _};
use std::time::{Duration, Instant};

/// How long one connection may take.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

fn address(host: &str, port: u16) -> Option<SocketAddr> {
    (host, port).to_socket_addrs().ok()?.next()
}

/// The durations, in microseconds, of `samples` TCP connections to `host:port`; failed ones are
/// left out, so an empty list means it did not answer.
#[must_use]
pub fn connect_times(host: &str, port: u16, samples: usize) -> Vec<u64> {
    let Some(addr) = address(host, port) else {
        return Vec::new();
    };
    (0..samples)
        .filter_map(|_| {
            let start = Instant::now();
            let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).ok()?;
            let elapsed = start.elapsed();
            drop(stream);
            Some(u64::try_from(elapsed.as_micros()).unwrap_or(u64::MAX))
        })
        .collect()
}

/// Whether a TCP connection to `host:port` succeeds.
#[must_use]
pub fn open(host: &str, port: u16) -> bool {
    address(host, port)
        .is_some_and(|addr| TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).is_ok())
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;

    use super::*;

    #[test]
    fn times_a_local_listener() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let times = connect_times("127.0.0.1", port, 3);
        assert_eq!(times.len(), 3);
        assert!(open("127.0.0.1", port));
        drop(listener);
        assert!(!open("127.0.0.1", port));
        assert_eq!(connect_times("127.0.0.1", port, 2).len(), 0);
    }
}
