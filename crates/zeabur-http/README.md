# zeabur-http

The one client for Zeabur's own outbound HTTP and HTTPS (images, object stores, GitHub).

- `Reach` decides which addresses the client may connect to. `Public` refuses loopback, link-local,
  unspecified, multicast and broadcast addresses, also as IPv4-mapped IPv6, after name resolution.
- `Proxies` reads `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`. HTTPS goes through an
  `http://` proxy with `CONNECT`. Local addresses are never proxied.
- The target is checked before a direct connection or `CONNECT`. For `Public`, the tunnel is asked
  for the checked address and TLS still uses the hostname, so the proxy's DNS cannot redirect it.
  The proxy hop itself is not checked.
- `tls` builds the TLS configuration from the platform's roots and an optional PEM bundle.
