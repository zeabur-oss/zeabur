# Profile: single-node

kubectl create namespace velero --dry-run=client -o yaml | kubectl apply -f -
kubectl label namespace velero --overwrite \
  pod-security.kubernetes.io/enforce=privileged \
  pod-security.kubernetes.io/audit=privileged \
  pod-security.kubernetes.io/warn=privileged
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
initContainers:
  # AWS plugin covers any S3-compatible store; v1.14.x matches Velero 1.18.
  - name: velero-plugin-for-aws
    image: velero/velero-plugin-for-aws:v1.14.4
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
  # Back up all volumes unless they opt out.
  defaultVolumesToFsBackup: true
# CSI snapshots need per-driver setup; use file-system backups instead.
snapshotsEnabled: false
# Needed for file-system backups.
deployNodeAgent: true
ZEABUR_VALUES
cat > velero-single-node.yaml <<'ZEABUR_VALUES'
resources:
  # Idle between backups; chart sets none.
  requests: {cpu: 20m, memory: 64Mi}
  limits: {memory: 512Mi}
ZEABUR_VALUES
helm upgrade --install velero velero \
  --repo https://vmware-tanzu.github.io/helm-charts \
  --version 12.2.1 \
  --namespace velero --create-namespace \
  --values velero-values.yaml \
  --values velero-single-node.yaml \
  --set configuration.backupStorageLocation.0.bucket='<bucket>' \
  --set configuration.backupStorageLocation.0.config.s3Url='<s3-url>'
