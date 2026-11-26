# zeabur-local

The local service: the API, acting with a person's kubeconfig, and the web interface, on a
loopback port of this machine. `zeaburctl ui` starts it with [`start`]; the desktop app starts one per cluster window with
[`start_from`], from the kubeconfig file the person picked.

## Hides

How the service starts and stops. [`start`] connects with one kubeconfig context, asks the
cluster who the person is, and listens on `127.0.0.1` on a port the system picks. Every request
goes through the guard of `zeabur_api::local`, so only the one-time link in [`Running::url`]
opens a session. [`Running::stop`] stops the server and waits for its open connections.

The service runs on a task of the Tokio runtime that calls [`start`]. The caller keeps its own
runtime and its own output.

[`telemetry`] is the experience program of a local run, shared by `zeaburctl ui` and the desktop
app: whether it is joined (`config.yaml`, `DO_NOT_TRACK`, `ZEABUR_TELEMETRY`), and the
`cluster_connected` event with [`ServiceError::connection`] and the `feature_used` events of the
service. Each event names its `client`, `desktop` or `zeaburctl`.

## Place in the workspace

Level 5: depends on `zeabur-api`, `zeabur-kube`, `zeabur-core`, `zeabur-telemetry` and, with `ui`, `zeabur-ui`.

## Features

- `ui`: serve the web interface (`zeabur-ui` with its `interface` feature) next to the API.
  Without it, the service serves the API alone.
