# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's private vulnerability reporting:
"Report a vulnerability" on the
[Security tab of zeabur/zeabur](https://github.com/zeabur/zeabur/security/advisories/new).
Do not open public issues, discussions or pull requests for security problems.

Useful in a report: the version or commit, how Zeabur runs (`zeaburctl ui` or in a cluster with
Zeabur Identity), the Kubernetes distribution and version, and the steps that show the problem. A
vulnerability in Zeabur Identity itself belongs to
[zeabur/identity](https://github.com/zeabur/identity/security/advisories/new).

## Supported versions

| Version | Supported |
| --- | --- |
| latest 0.0.x | yes |
| older releases | no |

Zeabur is pre-alpha, for evaluation on clusters you can afford to break. Only the latest 0.0.x
release receives security fixes, in a new release; fixes are not backported, and there is no
guaranteed response or patch timeline. Releases before 1.0.0 may change the API and the chart's
values without a migration path.
