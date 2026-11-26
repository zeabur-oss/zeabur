# Measuring application work

The experience program (docs/reference/telemetry.mdx) asks one question about application work:
does someone save a change in Zeabur OSS, and come back to save another? Zeabur OSS answers with
two events. Each one is a moment that happened, named after it, with no verdict of ours.

| Event | The moment | Properties |
|---|---|---|
| `cluster_connected` | `zeaburctl ui` connects to a cluster with a kubeconfig | `result`: `connected`, `unreachable`, `credential_refused`, `no_read_right`, `other` |
| `app_change_submitted` | Someone applies a plan, and Zeabur OSS answers | `result`: `accepted`, `forbidden`, `invalid`, `conflict`, `unreachable`, `other` |

One apply is one event, whatever it changes and however many objects it writes. What the change
was does not matter to the question, so the event does not say.

`result` is the class of the HTTP status the person got, whether the apply stopped before or
during the writes. A plan that someone else's change made stale is `conflict`. A plan token that
is expired, altered or malformed names no person, so it sends nothing. Zeabur OSS reads nothing
after it answers.

Every event also carries `zeabur_version` and `mode`; `app_change_submitted` carries the per-day
`person` hash of `zeabur_active`, so nothing links a person across days.

What these events add up to (weekly installations with an accepted save, where saves are
refused) is computed in PostHog, not here.
