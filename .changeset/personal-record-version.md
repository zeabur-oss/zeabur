---
default: patch
---

# A personal install records its KubeSolo version under `version`

A personal install now keeps the KubeSolo version in the `version` field of the install record, as K3s and RKE2 installs do, and the installer no longer reads the old `kubesolo` field. If you installed a personal server with a beta release, `apply` does not see the version it installed until you copy the field once. On the server, run:

```sh
sudo /var/lib/zeabur-installer/bin/kubectl --kubeconfig /var/lib/kubesolo/pki/admin/admin.kubeconfig \
  -n kube-system patch configmap zeabur-installer --type merge \
  -p "{\"data\":{\"version\":\"$(sudo /var/lib/zeabur-installer/bin/kubectl --kubeconfig /var/lib/kubesolo/pki/admin/admin.kubeconfig -n kube-system get configmap zeabur-installer -o jsonpath='{.data.kubesolo}')\"}}"
```

Running `zeabur-installer apply` again also writes the new field, so you can skip the command if you are about to apply.
