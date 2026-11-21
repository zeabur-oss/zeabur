---
default: patch
---

# A failed component install no longer fails `helm install`

The Job that installs `components.install` is no longer a Helm hook. `helm install` returns as soon as Zeabur OSS is deployed and prints the admin link, and the Job installs cert-manager after it. The Job retries for about 45 minutes, so a short outage of Zeabur Catalog heals by itself. When it succeeds, it deletes its `cluster-admin` binding. Its pods no longer carry the labels of the Zeabur OSS Service, so the Service never sends requests to them. Before, a catalog that Zeabur OSS could not read failed the release, and Helm printed no admin link.
