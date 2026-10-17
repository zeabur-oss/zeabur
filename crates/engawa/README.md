# engawa

`engawa`: the in-cluster server. It serves the API and the embedded web interface, and signs
people in through Genkan. Every request to the cluster carries the token of the person who made
it; Engawa's own service account is never used for one.

## Configuration

Flags, or the environment variable after each:

| Flag | Environment | |
|---|---|---|
| `--public-url` | `ENGAWA_PUBLIC_URL` | where people open Engawa, `https://` and the root of a host |
| `--issuer-url` | `ENGAWA_ISSUER_URL` | Genkan's issuer URL |
| `--issuer-ca-file` | `ENGAWA_ISSUER_CA_FILE` | a PEM file of extra CAs to trust for the issuer |
| `--client-id` | `ENGAWA_CLIENT_ID` | the `client_id` (default `engawa`) |
| `--client-secret-file` | `ENGAWA_CLIENT_SECRET_FILE` | a file with the client secret |
| `--plan-key-file` | `ENGAWA_PLAN_KEY_FILE` | a file with the plan-token key: 32 bytes, or 64 hex digits |
| `--elevation` | `ENGAWA_ELEVATION` | offer "Retry as admin" when the cluster refuses a request (`true` or `false`) |
| `--session-idle-minutes` | `ENGAWA_SESSION_IDLE_MINUTES` | default 30 |
| `--session-max-hours` | `ENGAWA_SESSION_MAX_HOURS` | default 8 |
| `--listen` | `ENGAWA_LISTEN` | default `[::]:8080`: IPv6 and, on Linux, IPv4 as well ([dual-stack](https://kubernetes.io/docs/concepts/services-networking/dual-stack/)); `0.0.0.0:8080` on a node without IPv6 |
| `--metrics-service` | `ENGAWA_METRICS_SERVICE` | the metrics store history is read from, `namespace/name:port` (default: where Engawa's VictoriaMetrics recommendation installs it) |
| `--namespace` | `ENGAWA_NAMESPACE` | the namespace Engawa runs in, whose ConfigMap `engawa-settings` keeps the settings people choose; unset, they cannot be changed |

Register Engawa in Genkan as a confidential client with the redirect URI
`<public URL>/auth/callback` and the audience kube-apiserver accepts (`--audience kubernetes`),
plus `--allow-elevation` when `--elevation` is on. The issuer's certificate is checked against
the image's CA bundle and, when given, `--issuer-ca-file` (for a private CA). A bad value stops the server at start
with a message naming the flag.

Run one replica: sessions live in memory (see `engawa_auth::session`).

## Features

- `ui` (default): embed the web interface (`engawa-ui`). `--no-default-features` builds the API
  only and needs no JavaScript toolchain.
