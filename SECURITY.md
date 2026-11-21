# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through GitHub's private vulnerability reporting:
"Report a vulnerability" on the
[Security tab of zeabur-oss/zeabur](https://github.com/zeabur-oss/zeabur/security/advisories/new).
Do not open public issues, discussions or pull requests for security problems.

Useful in a report: the version or commit, how Zeabur OSS runs (`zeaburctl ui`, or in a cluster with
Zeabur Identity), the Kubernetes distribution and version, and the steps that show the problem. A
vulnerability in Zeabur Identity itself belongs to
[zeabur-oss/identity](https://github.com/zeabur-oss/identity/security/advisories/new).

## Supported versions

| Version | Supported |
| --- | --- |
| latest stable release | yes |
| older releases and prereleases | no |

Only the latest stable release receives security fixes, in a new release; fixes are not
backported.
