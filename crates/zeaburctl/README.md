# zeaburctl

`zeaburctl`: the command-line tool for people using a cluster.

- `zeaburctl ui`: the web interface on this machine, with the current kubeconfig context.
- `zeaburctl doctor`: checks the cluster version, the Zeabur Identity setup and recommended components.

## `zeaburctl doctor`

`zeaburctl doctor [--context <name>] [--output text|json]` reads the same API the web interface
uses, in-process and with your kubeconfig, so it sees what your RBAC lets you see. A check you
may not read is `skipped`, naming the rule it needs; it does not fail the run. Whether
kube-apiserver trusts Zeabur Identity's issuer cannot be read through RBAC, so it is reported as not
checked.

`--output json` prints `{"checks": [{"name", "status", "findings", "fix"}]}`, with `status` one
of `ok`, `warning`, `critical`, `skipped` and `error`, and `fix` `null` when there is nothing to
do.

The exit code follows the
[Nagios plugin convention](https://nagios-plugins.org/doc/guidelines.html), taking the most
serious result in the order critical, warning, could not run:

| Code | Meaning |
|---|---|
| 0 | nothing to fix (every check `ok` or `skipped`) |
| 1 | a `warning` |
| 2 | something `critical` |
| 3 | the cluster cannot be reached, or a check could not run (`error`) |

clap's usage error (an unknown flag, a bad `--output`) is also 2: tell it apart by the message on
stderr and the empty stdout.

## Features

- `ui` (default): embed the web interface (`zeabur-ui`). `--no-default-features` builds without
  it and needs no JavaScript toolchain.
