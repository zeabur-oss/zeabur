# zeabur-installer-host

What `zeabur-installer` does to the host: reading facts, preflight checks, verified downloads,
files, systemd and `kubectl`. Each operation is small and safe to repeat, so an interrupted
`apply` resumes by running again.
