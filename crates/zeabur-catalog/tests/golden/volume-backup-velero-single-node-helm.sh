# Profile: single-node

kubectl create namespace velero --dry-run=client -o yaml | kubectl apply -f -
# Velero reads the bucket's credentials from this AWS-style credentials file.
kubectl -n velero create secret generic velero-credentials \
  --from-literal=cloud='<credentials>' \
  --dry-run=client -o yaml | kubectl apply -f -
# Kopia encrypts file-system backups with this password. Without the Secret, Velero uses its built-in password, which is public.
kubectl -n velero create secret generic velero-repo-credentials \
  --from-literal=repository-password="$(openssl rand -hex 64)" \
  --dry-run=client -o yaml | kubectl apply -f -
# The S3 server encrypts every backup object, Secrets included, with this SSE-C key. The AWS plugin needs exactly 32 bytes.
kubectl -n velero create secret generic velero-backup-key \
  --from-literal=key="$(openssl rand -base64 24 | tr '+/' '-_' | tr -d '=')" \
  --dry-run=client -o yaml | kubectl apply -f -

# velero
cat > velero-values.yaml <<'ZEABUR_VALUES'
# Pinned by digest; a tag alone can move.
image:
  repository: docker.io/velero/velero
  tag: v1.18.4@sha256:c89fb5b6d1fd6afd368e0f483e6f5555fd62851a1eada8ac5c72482e674ca17b
# The chart sets no security context; the namespace enforces Pod Security restricted.
# The image runs as the name cnb, so runAsNonRoot needs its numeric ids from /etc/passwd.
podSecurityContext:
  runAsNonRoot: true
  runAsUser: 1002
  runAsGroup: 1000
  seccompProfile: {type: RuntimeDefault}
# Pod Security restricted requires both settings.
containerSecurityContext:
  allowPrivilegeEscalation: false
  capabilities: {drop: [ALL]}
initContainers:
  # AWS plugin covers any S3-compatible store; v1.14.x matches Velero 1.18.
  - name: velero-plugin-for-aws
    image: velero/velero-plugin-for-aws:v1.14.4@sha256:c85a7fa803569e3128c33e4f5766f84ab156c8d171c9f1dd2715581d45ce98fa
    securityContext:
      allowPrivilegeEscalation: false
      capabilities: {drop: [ALL]}
    volumeMounts:
      - mountPath: /target
        name: plugins
credentials:
  # Secret comes from the credentials input, not the chart.
  useSecret: true
  existingSecret: velero-credentials
configuration:
  backupStorageLocation:
    # One S3 location; bucket and URL are inputs.
    - name: default
      provider: aws
      default: true
      config:
        region: us-east-1
        s3ForcePathStyle: "true"
        # Velero writes Secrets into the backup as they are. The
        # store encrypts them with the backup-key input (SSE-C).
        customerKeyEncryptionSecret: velero-backup-key/key
  # Kopia file-system backups work on any storage class, local-path included.
  uploaderType: kopia
  # No node agent by default, so no volume copy. Backups opt in with file-backup.
  defaultVolumesToFsBackup: false
# CSI snapshots need per-driver setup; use file-system backups instead.
snapshotsEnabled: false
ZEABUR_VALUES
cat > velero-single-node.yaml <<'ZEABUR_VALUES'
resources:
  # Idle between backups; chart sets none.
  requests: {cpu: 20m, memory: 64Mi}
  limits: {memory: 512Mi}
ZEABUR_VALUES
# Helm replaces a list as a whole, so this file repeats each list an input is in.
cat > velero-inputs.yaml <<'ZEABUR_VALUES'
configuration:
  backupStorageLocation:
  - bucket: <bucket>
    config:
      customerKeyEncryptionSecret: velero-backup-key/key
      region: us-east-1
      s3ForcePathStyle: "true"
      s3Url: <s3-url>
    default: true
    name: default
    provider: aws
ZEABUR_VALUES
helm upgrade --install velero velero \
  --repo https://vmware-tanzu.github.io/helm-charts \
  --version 12.2.1 \
  --namespace velero --create-namespace \
  --values velero-values.yaml \
  --values velero-single-node.yaml \
  --values velero-inputs.yaml \
  --set deployNodeAgent='<file-backup>'
