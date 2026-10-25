# Zeabur

`zeabur`: the in-cluster server. It serves the API and the embedded web interface, and signs
people in through Zeabur Identity. Every request to the cluster carries the token of the person who made
it. Zeabur's own service account is used only for tools pods in the terminal namespace, after
kube-apiserver says the person may run commands in the project's pods (see `zeabur_api::tools`).

## Configuration

Flags, or the environment variable after each:

| Flag | Environment | |
|---|---|---|
| `--public-url` | `ZEABUR_PUBLIC_URL` | where people open Zeabur, `https://` and the root of a host |
| `--issuer-url` | `ZEABUR_ISSUER_URL` | Zeabur Identity's issuer URL |
| `--issuer-ca-file` | `ZEABUR_ISSUER_CA_FILE` | a PEM file of extra CAs to trust for the issuer only |
| `--extra-ca-file` | `ZEABUR_EXTRA_CA_FILE` | a PEM file of extra CAs to trust for every HTTPS connection, the issuer included |
| `--client-id` | `ZEABUR_CLIENT_ID` | the `client_id` (default `zeabur`) |
| `--client-secret-file` | `ZEABUR_CLIENT_SECRET_FILE` | a file with the client secret |
| `--plan-key-file` | `ZEABUR_PLAN_KEY_FILE` | a file with the plan-token key: 32 bytes, or 64 hex digits |
| `--session-idle-minutes` | `ZEABUR_SESSION_IDLE_MINUTES` | default 60; Zeabur Identity's idle limit for the account applies when shorter |
| `--session-max-hours` | `ZEABUR_SESSION_MAX_HOURS` | default 12 |
| `--listen` | `ZEABUR_LISTEN` | default `[::]:8080`: IPv6 and, on Linux, IPv4 as well ([dual-stack](https://kubernetes.io/docs/concepts/services-networking/dual-stack/)); `0.0.0.0:8080` on a node without IPv6 |
| `--metrics-service` | `ZEABUR_METRICS_SERVICE` | the metrics store history, node disk and volume use are read from, `namespace/name:port` (default: where Zeabur's VictoriaMetrics recommendation installs it) |
| `--namespace` | `ZEABUR_NAMESPACE` | the namespace Zeabur runs in, whose ConfigMap `zeabur-settings` keeps the settings people choose; unset, they cannot be changed |
| `--terminal-namespace` | `ZEABUR_TERMINAL_NAMESPACE` | the namespace all kubectl terminal (tools) pods go in (default `zeabur-terminals`); give members no rights there |
| `--terminal-token-file` | `ZEABUR_TERMINAL_TOKEN_FILE` | a file with Zeabur's own service account token, which may use pods in the terminal namespace only; unset, there are no kubectl terminals |

Register Zeabur in Zeabur Identity as a confidential client with the redirect URI
`<public URL>/auth/callback` and the audience kube-apiserver accepts (`--audience kubernetes`).
The issuer's certificate is checked against
the image's CA bundle and, when given, `--issuer-ca-file` and `--extra-ca-file` (for a private CA). A bad value stops the server at start
with a message naming the flag.

Run one replica: sessions live in memory (see `zeabur_auth::session`).

## Features

- `ui` (default): embed the web interface (`zeabur-ui`). `--no-default-features` builds the API
  only and needs no JavaScript toolchain.
