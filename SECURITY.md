# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's private vulnerability reporting:
"Report a vulnerability" on the
[Security tab of engawahq/engawa](https://github.com/engawahq/engawa/security/advisories/new).
Do not open public issues, discussions or pull requests for security problems.

Useful in a report: the version or commit, how Engawa runs (`engawactl ui` or in a cluster with
Genkan), the Kubernetes distribution and version, and the steps that show the problem. A
vulnerability in Genkan itself belongs to
[engawahq/genkan](https://github.com/engawahq/genkan/security/advisories/new).

## Supported versions

| Version | Supported |
| --- | --- |
| latest 0.0.x | yes |
| older releases | no |

Engawa is pre-alpha, for evaluation on clusters you can afford to break. Only the latest 0.0.x
release receives security fixes, in a new release; fixes are not backported, and there is no
guaranteed response or patch timeline. Releases before 1.0.0 may change the API and the chart's
values without a migration path.
