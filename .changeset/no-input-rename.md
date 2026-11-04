---
default: major
---

# The installer no longer renames old component input names

`apply` and `gui` no longer rewrite old input names in the configuration file, and no command reads an old name as the new one. If your `inputs` or `secretFiles` still use an old name, rename the key by hand before you upgrade:

- `builds/zeabur-builder`: `cluster-ip` → `clusterIp`
- `intrusion-prevention/crowdsec`: `bouncer-key` → `bouncerKey`, `lapi-secret` → `lapiSecret`, `registration-token` → `registrationToken`
- `remote-access/tailscale-operator`: `oauth-client-id` → `oauthClientId`, `oauth-client-secret` → `oauthClientSecret`
- `volume-backup/velero`: `s3-url` → `s3Url`, `repository-password` → `repositoryPassword`, `backup-key` → `backupKey`, `file-backup` → `fileBackup`
