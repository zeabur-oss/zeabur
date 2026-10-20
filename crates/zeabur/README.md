# Zeabur

`zeabur`: the in-cluster server. It serves the API and the embedded web interface, and signs
people in through Zeabur Identity. Every request to the cluster carries the token of the person who made
it; Zeabur's own service account is never used for one.

## Configuration

Flags, or the environment variable after each:

| Flag | Environment | |
|---|---|---|
| `--public-url` | `ZEABUR_PUBLIC_URL` | where people open Zeabur, `https://` and the root of a host |
| `--issuer-url` | `ZEABUR_ISSUER_URL` | Zeabur Identity's issuer URL |
| `--issuer-ca-file` | `ZEABUR_ISSUER_CA_FILE` | a PEM file of extra CAs to trust for the issuer |
| `--client-id` | `ZEABUR_CLIENT_ID` | the `client_id` (default `zeabur`) |
| `--client-secret-file` | `ZEABUR_CLIENT_SECRET_FILE` | a file with the client secret |
| `--plan-key-file` | `ZEABUR_PLAN_KEY_FILE` | a file with the plan-token key: 32 bytes, or 64 hex digits |
| `--elevation` | `ZEABUR_ELEVATION` | offer "Retry as admin" when the cluster refuses a request (`true` or `false`) |
| `--session-idle-minutes` | `ZEABUR_SESSION_IDLE_MINUTES` | default 30 |
| `--session-max-hours` | `ZEABUR_SESSION_MAX_HOURS` | default 8 |
| `--listen` | `ZEABUR_LISTEN` | default `[::]:8080`: IPv6 and, on Linux, IPv4 as well ([dual-stack](https://kubernetes.io/docs/concepts/services-networking/dual-stack/)); `0.0.0.0:8080` on a node without IPv6 |
| `--metrics-service` | `ZEABUR_METRICS_SERVICE` | the metrics store history is read from, `namespace/name:port` (default: where Zeabur's VictoriaMetrics recommendation installs it) |
| `--namespace` | `ZEABUR_NAMESPACE` | the namespace Zeabur runs in, whose ConfigMap `zeabur-settings` keeps the settings people choose; unset, they cannot be changed |

Register Zeabur in Zeabur Identity as a confidential client with the redirect URI
`<public URL>/auth/callback` and the audience kube-apiserver accepts (`--audience kubernetes`),
plus `--allow-elevation` when `--elevation` is on. The issuer's certificate is checked against
the image's CA bundle and, when given, `--issuer-ca-file` (for a private CA). A bad value stops the server at start
with a message naming the flag.

Run one replica: sessions live in memory (see `zeabur_auth::session`).

## Features

- `ui` (default): embed the web interface (`zeabur-ui`). `--no-default-features` builds the API
  only and needs no JavaScript toolchain.
