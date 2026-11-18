# Measuring application work

The experience program (docs/reference/telemetry.mdx) asks one question about application work:
does someone finish a change in Zeabur, and come back to make another? Zeabur answers with three
events. Each one is a moment that happened, named after it, with no verdict of ours.

| Event | The moment | Properties |
|---|---|---|
| `cluster_connected` | `zeaburctl ui` connects to a cluster with a kubeconfig | `result`: `connected`, `unreachable`, `credential_refused`, `no_read_right`, `other` |
| `app_change_submitted` | Zeabur sends a change of one Deployment to the cluster and answers the person | `change_id`, `operation`, `result`: `accepted`, `forbidden`, `invalid`, `conflict`, `unreachable`, `other` |
| `rollout_finished` | The rollout of an accepted change ends | `change_id`, `operation`, `result`: `available`, `progress_deadline_exceeded` |

`operation` is `deploy_image`, `scale_replicas` or `change_env`. A change counts only when it is
exactly one of these on one Deployment.

`result` keeps the source's own words. For `app_change_submitted` it is the class of the HTTP
status the person got. For `rollout_finished` it is what `kubectl rollout status` reports: every
replica runs the change and is available, or the Deployment's `Progressing` condition turned
`False` with the reason `ProgressDeadlineExceeded` after its `progressDeadlineSeconds`.

After an accepted change, Zeabur reads the Deployment as the person until one of those two
happens. The rollout belongs to the Deployment's `uid` and the `generation` the write left, so a
Deployment deleted and made again with the same name is not the same change. Each read has a time
limit, and the whole watch ends at the progress deadline plus a minute, never later than 30
minutes after the change.

An accepted change can end with no `rollout_finished`:

- a newer change replaced it,
- the person signed out or can no longer read the Deployment,
- the Deployment was deleted or made again,
- the watch ran out of time,
- Zeabur restarted, which forgets the change,
- or the event was not delivered.

A missing `rollout_finished` therefore means "no result", never success or failure. Every view
of rollouts shows three outcomes of accepted changes side by side, for each `operation`:
`available`, `progress_deadline_exceeded` and no result, and the coverage, the share of accepted
changes that have a `rollout_finished`. A success rate counted only over changes with a result
leaves out the missing ones, so it is shown only next to its coverage.

Every event also carries `zeabur_version` and `mode`; the two change events carry
the per-day `person` hash of `zeabur_active`, for the day the change was submitted. A rollout
that ends after midnight UTC keeps that day's hash, so nothing links a person across days.

What these events add up to (weekly returning instances, time to a first available rollout,
where changes stop) is computed in PostHog, not here.
