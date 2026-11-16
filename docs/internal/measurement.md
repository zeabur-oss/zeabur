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
happens. It stops without an event when a newer change replaces this one, when the person can no
longer read the Deployment, or when the progress deadline has passed by a minute with neither
result. A restart forgets the change. A missing `rollout_finished` therefore means "no result",
and the analysis treats it so.

Every event also carries `zeabur_version`, `mode` and `is_internal`; the two change events carry
the per-day `person` hash of `zeabur_active`. Nothing links a person across days.

What these events add up to (weekly returning instances, time to a first available rollout,
where changes stop) is computed in PostHog, not here.
