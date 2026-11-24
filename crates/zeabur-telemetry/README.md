# zeabur-telemetry

The usage data Zeabur OSS may send, and the one sender for it. An
[`Event`] holds only numbers, enum strings and the random [`InstallationId`]; there is no field
for free text, so nothing else can be added by accident. The page that lists every event and
property for readers is `docs/reference/telemetry.mdx`.

A [`Sender`] sends nothing unless its [`Switch`] is `On`. `DO_NOT_TRACK` (any value but empty or
`0`) turns it off whatever else is set; `ZEABUR_TELEMETRY=off` does too, and
`ZEABUR_TELEMETRY=debug` hands the exact request body to a callback instead of sending it.

[`Sender::send`] queues an event and returns at once; a worker on the Tokio runtime posts the
queue in batches. Only a 2xx answer counts as sent. A 429, a 5xx or a network error is retried
three times with backoff, honouring `Retry-After`; another answer drops the batch with a warning.
A full queue drops the event. [`Sender::close`] waits, up to a limit, for the queue to be sent:
call it before the process exits.

A local run has no installation ID. [`Id::Local`] reads its [`LocalId`], a file in the
platform's configuration directory, and makes it only when a request is about to be posted.

[`do_not_track`] is the one reading of `DO_NOT_TRACK`; the update checks of the interface and
`zeaburctl` use it too.
