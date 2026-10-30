# zeabur-telemetry

The usage data Zeabur and `zeabur-installer` may send, and the one sender they share. An
[`Event`] holds only numbers, enum strings and the random [`InstallationId`]; there is no field
for free text, so nothing else can be added by accident. The page that lists every event and
property for readers is `docs/reference/telemetry.mdx`.

A [`Sender`] sends nothing unless its [`Switch`] is `On`. `DO_NOT_TRACK` (any value but empty or
`0`) turns it off whatever else is set; `ZEABUR_TELEMETRY=off` does too, and
`ZEABUR_TELEMETRY=debug` hands the exact request body to a callback instead of sending it. A
send is one try with a short timeout; a failure is a debug log line and nothing else.
