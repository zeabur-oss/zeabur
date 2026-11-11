use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bytes::Bytes;
use http::Request;
use http_body_util::Full;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpListener;

use super::*;

#[test]
fn the_server_never_reaches_its_own_host_or_the_metadata_service() {
    for refused in [
        "127.0.0.1",
        "169.254.169.254",
        "0.0.0.0",
        "::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "224.0.0.1",
        "255.255.255.255",
    ] {
        let ip: IpAddr = refused.parse().unwrap();
        assert!(!allowed(Reach::Public, ip), "{refused}");
        assert!(allowed(Reach::Any, ip), "{refused}");
    }
    // Registries inside the cluster are on private addresses.
    for kept in ["10.43.0.10", "192.168.1.5", "104.16.0.1", "2606:4700::1"] {
        assert!(allowed(Reach::Public, kept.parse().unwrap()), "{kept}");
    }
}

#[test]
fn an_extra_ca_must_be_a_pem_certificate_bundle() {
    let error = tls(Some(b"not a certificate")).unwrap_err();
    assert!(error.contains("not a PEM certificate bundle"), "{error}");
    assert!(tls(Some(b"")).is_err());
    assert!(tls(None).is_ok());
}

#[tokio::test]
async fn a_url_that_names_a_refused_address_is_not_sent() {
    let client = Https::new(Reach::Public, &tls(None).unwrap());
    let error = get(&client, "http://169.254.169.254/latest/meta-data/")
        .await
        .unwrap_err();
    assert_eq!(error, "Zeabur does not connect to 169.254.169.254");
}

#[tokio::test]
async fn a_pooled_connection_is_closed_once_the_idle_timeout_passes() {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0; 1024];
        let _request = socket.read(&mut buffer).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buffer))
            .await
            .map(|read| read.unwrap())
    });
    let client = pooled(
        connector(Reach::Any, &tls(None).unwrap()),
        Duration::from_millis(100),
    );
    let request = Request::get(url).body(Full::<Bytes>::default()).unwrap();
    let response = client.request(request).await.unwrap();
    response.into_body().collect().await.unwrap();
    assert_eq!(server.await.unwrap(), Ok(0), "the server saw the close");
}

async fn exchange_over_tls(extra_ca: Option<&str>) -> Result<u16, String> {
    let address = zeabur_test_support::tls::serve().await;
    let client = Https::new(Reach::Any, &tls(extra_ca.map(str::as_bytes)).unwrap());
    let request = Request::get(format!("https://{address}/"))
        .body(Full::default())
        .unwrap();
    client
        .exchange(request, 1024)
        .await
        .map(|(status, _)| status)
}

#[tokio::test]
async fn a_certificate_is_trusted_only_under_the_trusted_ca_that_signed_it() {
    use zeabur_test_support::tls::{ca_a, ca_b};

    assert_eq!(exchange_over_tls(Some(ca_a())).await, Ok(200));
    assert!(exchange_over_tls(Some(ca_b())).await.is_err());
    assert!(exchange_over_tls(None).await.is_err());
}

/// The request lines a fake proxy saw, and the authority it was asked to tunnel to.
type Seen = Arc<Mutex<Vec<String>>>;

/// A proxy that answers `CONNECT` with `status`, and on 200 tunnels to `onward`.
async fn fake_proxy(status: u16, onward: Option<SocketAddr>) -> (SocketAddr, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen: Seen = Arc::default();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let (mut client, _) = listener.accept().await.unwrap();
            let log = Arc::clone(&log);
            tokio::spawn(async move {
                let mut head = Vec::new();
                while !head.ends_with(b"\r\n\r\n") {
                    head.push(client.read_u8().await.unwrap());
                }
                log.lock().unwrap().push(String::from_utf8(head).unwrap());
                if status != 200 {
                    let line = format!("HTTP/1.1 {status} No\r\ncontent-length: 0\r\n\r\n");
                    client.write_all(line.as_bytes()).await.unwrap();
                    return;
                }
                client
                    .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                    .await
                    .unwrap();
                if let Some(onward) = onward {
                    let mut upstream = tokio::net::TcpStream::connect(onward).await.unwrap();
                    let _copied = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
                }
            });
        }
    });
    (address, seen)
}

fn through(proxy: SocketAddr, no_proxy: &str) -> Proxies {
    Proxies::all(&format!("http://{proxy}"), no_proxy)
}

fn resolving_to(ip: &'static str) -> connect::Resolver {
    Arc::new(move |_host, port| {
        Box::pin(async move { Ok(vec![SocketAddr::new(ip.parse().unwrap(), port)]) })
    })
}

fn client(reach: Reach, proxies: Proxies, resolve: connect::Resolver) -> Https {
    let connector = connector_with(reach, &tls(None).unwrap(), proxies).resolving(resolve);
    Https {
        client: pooled(connector, IDLE),
        reach,
    }
}

async fn get(client: &Https, url: &str) -> Result<(u16, Vec<u8>), String> {
    let request = Request::get(url).body(Full::<Bytes>::default()).unwrap();
    client.exchange(request, 1024).await
}

#[tokio::test]
async fn an_https_destination_goes_through_the_proxy_to_its_checked_address() {
    let target = zeabur_test_support::tls::serve().await;
    let (proxy, seen) = fake_proxy(200, Some(target)).await;
    // registry.test resolves to a public-looking address the proxy never dials; the tunnel is
    // asked for that address, and the fake proxy forwards to the TLS server.
    let ca = zeabur_test_support::tls::ca_a();
    let tls = tls(Some(ca.as_bytes())).unwrap();
    let connector = connector_with(Reach::Public, &tls, through(proxy, ""))
        .resolving(resolving_to("203.0.113.7"));
    let https = Https {
        client: pooled(connector, IDLE),
        reach: Reach::Public,
    };
    let (status, body) = get(&https, "https://registry.test/").await.unwrap();
    assert_eq!((status, body.as_slice()), (200, b"ok".as_slice()));
    let head = seen.lock().unwrap().remove(0);
    assert!(
        head.starts_with("CONNECT 203.0.113.7:443 HTTP/1.1"),
        "{head}"
    );
}

#[tokio::test]
async fn a_name_that_resolves_to_a_refused_address_is_not_tunnelled() {
    let (proxy, seen) = fake_proxy(200, None).await;
    let https = client(
        Reach::Public,
        through(proxy, ""),
        resolving_to("169.254.169.254"),
    );
    let error = get(&https, "http://registry.test/").await.unwrap_err();
    assert!(
        error.contains("does not connect to the addresses of registry.test"),
        "{error}"
    );
    assert!(seen.lock().unwrap().is_empty(), "the proxy was not asked");
}

#[tokio::test]
async fn no_proxy_and_local_addresses_are_reached_directly() {
    let (proxy, seen) = fake_proxy(200, None).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0; 1024];
            let _read = socket.read(&mut buffer).await.unwrap();
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .await
                .unwrap();
        }
    });
    let https = client(Reach::Any, through(proxy, ""), resolving_to("127.0.0.1"));
    let local = get(&https, &format!("http://{address}/")).await.unwrap();
    assert_eq!(local.0, 200);

    let excluded = client(
        Reach::Any,
        through(proxy, ".test"),
        resolving_to("127.0.0.1"),
    );
    let named = get(
        &excluded,
        &format!("http://registry.test:{}/", address.port()),
    )
    .await
    .unwrap();
    assert_eq!(named.0, 200);
    assert!(
        seen.lock().unwrap().is_empty(),
        "nothing went through the proxy"
    );
}

#[tokio::test]
async fn a_proxy_that_refuses_the_tunnel_fails_the_request() {
    let (proxy, _) = fake_proxy(403, None).await;
    let https = client(
        Reach::Public,
        through(proxy, ""),
        resolving_to("203.0.113.7"),
    );
    let error = get(&https, "http://registry.test/").await.unwrap_err();
    assert!(error.contains("status 403"), "{error}");
}

#[tokio::test]
async fn proxy_credentials_are_sent_to_the_proxy_only() {
    let (proxy, seen) = fake_proxy(403, None).await;
    let proxies = Proxies::all(&format!("http://user:pass@{proxy}"), "");
    let https = client(Reach::Public, proxies, resolving_to("203.0.113.7"));
    let _refused = get(&https, "http://registry.test/").await;
    let head = seen.lock().unwrap().remove(0);
    assert!(
        head.contains("Proxy-Authorization: Basic dXNlcjpwYXNz"),
        "{head}"
    );
}

#[test]
fn local_destinations_are_never_proxied() {
    for local in [
        "localhost",
        "a.localhost",
        "127.0.0.1",
        "::1",
        "0.0.0.0",
        "169.254.1.1",
        "::ffff:127.0.0.1",
    ] {
        assert!(connect::is_local(local), "{local}");
    }
    for remote in ["registry.test", "10.43.0.10", "104.16.0.1"] {
        assert!(!connect::is_local(remote), "{remote}");
    }
}

#[test]
fn the_charts_bypass_list_keeps_cluster_destinations_direct() {
    let proxies = Proxies::all(
        "http://proxy.example.test:3128",
        "localhost,.svc,.cluster.local,10.42.0.0/16,10.43.0.0/16,.corp.example.test",
    );
    let connector = connector_with(Reach::Public, &tls(None).unwrap(), proxies);
    let routed = |url: &str| connector.route("", &url.parse().unwrap()).is_some();
    for direct in [
        "https://zot.zot.svc:5000/v2/",
        "https://api.kube.cluster.local/",
        "https://10.43.0.10:5000/v2/",
        "https://10.42.3.4/",
        "https://registry.corp.example.test/",
    ] {
        assert!(!routed(direct), "{direct}");
    }
    for proxied in [
        "https://ghcr.io/v2/",
        "https://api.github.com/",
        "https://203.0.113.7/",
    ] {
        assert!(routed(proxied), "{proxied}");
    }
}

#[tokio::test]
async fn forbidden_literal_targets_are_refused_before_the_proxy_is_asked() {
    let (proxy, seen) = fake_proxy(200, None).await;
    let https = client(
        Reach::Public,
        through(proxy, ""),
        resolving_to("203.0.113.7"),
    );
    for url in [
        "https://169.254.169.254/latest/",
        "https://[::1]/",
        "https://[::ffff:169.254.169.254]/",
        "https://[fe80::1]/",
    ] {
        let error = get(&https, url).await.unwrap_err();
        assert!(error.contains("does not connect to"), "{url}: {error}");
    }
    assert!(seen.lock().unwrap().is_empty(), "the proxy was not asked");
}

#[tokio::test]
async fn the_next_address_is_tried_when_the_first_one_does_not_connect() {
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let closed_port = closed.local_addr().unwrap().port();
    drop(closed);
    let live = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let live_address = live.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = live.accept().await.unwrap();
        let mut buffer = [0; 1024];
        let _read = socket.read(&mut buffer).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
            .await
            .unwrap();
    });
    let addresses = vec![
        SocketAddr::from(([127, 0, 0, 1], closed_port)),
        live_address,
    ];
    let resolve: connect::Resolver = Arc::new(move |_host, _port| {
        let addresses = addresses.clone();
        Box::pin(async move { Ok(addresses) })
    });
    let https = client(Reach::Any, Proxies::none(), resolve);
    let (status, _) = get(&https, "http://registry.test/").await.unwrap();
    assert_eq!(status, 200);
}

#[tokio::test]
async fn the_next_checked_address_is_asked_of_the_proxy_when_the_first_is_refused() {
    let target = zeabur_test_support::tls::serve().await;
    let (proxy, seen) = fake_proxy_refusing_first(target).await;
    let tls = tls(Some(zeabur_test_support::tls::ca_a().as_bytes())).unwrap();
    let resolve: connect::Resolver = Arc::new(|_host, port| {
        Box::pin(async move {
            Ok(vec![
                SocketAddr::new("203.0.113.7".parse().unwrap(), port),
                SocketAddr::new("203.0.113.8".parse().unwrap(), port),
            ])
        })
    });
    let connector = connector_with(Reach::Public, &tls, through(proxy, "")).resolving(resolve);
    let https = Https {
        client: pooled(connector, IDLE),
        reach: Reach::Public,
    };
    let (status, _) = get(&https, "https://registry.test/").await.unwrap();
    assert_eq!(status, 200);
    let heads = seen.lock().unwrap().clone();
    assert!(heads[0].starts_with("CONNECT 203.0.113.7:443"), "{heads:?}");
    assert!(heads[1].starts_with("CONNECT 203.0.113.8:443"), "{heads:?}");
}

/// A proxy that refuses the first `CONNECT` it gets and tunnels the rest to `onward`.
async fn fake_proxy_refusing_first(onward: SocketAddr) -> (SocketAddr, Seen) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen: Seen = Arc::default();
    let log = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let (mut client, _) = listener.accept().await.unwrap();
            let mut head = Vec::new();
            while !head.ends_with(b"\r\n\r\n") {
                head.push(client.read_u8().await.unwrap());
            }
            let first = {
                let mut log = log.lock().unwrap();
                log.push(String::from_utf8(head).unwrap());
                log.len() == 1
            };
            if first {
                client
                    .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
                    .await
                    .unwrap();
                continue;
            }
            client
                .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                .await
                .unwrap();
            tokio::spawn(async move {
                let mut upstream = tokio::net::TcpStream::connect(onward).await.unwrap();
                let _copied = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
            });
        }
    });
    (address, seen)
}
