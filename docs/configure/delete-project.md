# Delete a project

A project is a Kubernetes namespace. **Delete project**, in the **Danger zone** of a project's
**Settings**, deletes that namespace and everything in it: services, databases, volumes,
settings and secrets, as `kubectl delete namespace` does. It can't be undone; you can only bring
the project back from a backup.

To stop showing a namespace in Zeabur without deleting anything, use **Remove from Zeabur** in the
same place. It removes the project label only.

## Before you start

- You need the right to delete the namespace (`delete namespaces/<project>`) and the database
  objects in it. The review lists any right you lack, and the cluster refuses the apply.
- Zeabur's own namespaces, `kube-*` namespaces, components the installer put in (labelled
  `installer.zeabur.com/baseline=true`) and `zeabur-terminals` can't be deleted here.

## Review what goes

**Goes with the project** lists:

- The services, by name.
- Each database, with the time of its last backup.
- Each volume, with the time of its last backup and whether its data stays. When the volume's
  PersistentVolume says `Retain`, the data stays in that PersistentVolume; `Delete` deletes it.
  When Zeabur can't read the PersistentVolume it reads the StorageClass's `reclaimPolicy`, and when
  it can read neither it says it can't tell.
- Your tools pods for the project. They run in `zeabur-terminals`; Zeabur deletes them after the
  namespace.

A database or volume with no successful backup in the last day gets **Back up first**. It swaps
the review for that backup's, using the same **Back up now** as the database page and volume
backups; apply it, wait for the backup to finish, then come back to delete the project. It shows
only where backups are set up.

**Stays** lists the backups in S3 and everything outside the project, such as other projects and
cluster settings.

K8up volume backups need their restic password to restore, and it's in a Secret in the project
(for example `data-redis-0-backup`) that goes with it. To restore those volumes later, keep a copy
first:

```sh
kubectl -n shop get secret data-redis-0-backup -o jsonpath='{.data.RESTIC_PASSWORD}' | base64 -d
```

A cluster backup (Velero) holds the whole namespace, Secrets included, so restoring from one needs
nothing kept aside.

When Helm, Argo CD or Flux manages objects in the project, the review says so: the tool may create
them again after the deletion. Remove the application from the tool first.

Type the project name, or click the name in the prompt to fill it in, then press **Delete project**.

## What happens

Zeabur deletes as you, in this order:

1. The database objects, such as CloudNativePG's `Cluster`, so each operator can clean up its way.
   While the operator runs, Zeabur waits up to a minute for the database to go. When the review
   says a database's operator isn't running, that cleanup won't happen and the project may stay
   in deletion.
2. The namespace. Kubernetes then deletes what is left in it.
3. Your tools pods for the project.

Meanwhile the projects list shows **Deleting…** and the project page says what is left. It usually
takes a minute or two.

## The project doesn't go away

A namespace waits until every object in it has lost its finalizers. The controller that added a
finalizer removes it after its cleanup; while that controller doesn't run, the object stays.

The project page lists the objects that still have finalizers and the commands to look at them.
For a project `shop`:

```sh
kubectl get namespace shop -o yaml
kubectl get clusters.postgresql.cnpg.io orders -n shop -o yaml
```

`status.conditions` says which resources are left and which finalizers they wait for. Usually the
fix is to get the operator running again; it removes its finalizer when its cleanup is done. Zeabur
never removes a finalizer: that skips the cleanup and can leave resources outside the cluster,
such as cloud disks or files in S3.

## Bring a project back

In **Cluster** → **Backups**, pick a backup from before the deletion and press **Restore deleted
project** (**New project from backup** while the project still exists). See the cluster backups
page for the steps.

Databases' and volumes' own S3 backups stay too: restore them into a new database or volume in a
new project.

## The same with kubectl

```sh
kubectl delete clusters.postgresql.cnpg.io --all -n shop   # one line per database kind, first
kubectl delete namespace shop
```

`kubectl delete namespace shop` alone deletes everything too; the difference is that databases go
at the same time as everything else, and their operators may not get to clean up.

A tools pod's project is in its `zeabur.com/tools-project` annotation, which a label selector can't
match. List the pods of `shop` in `zeabur-terminals`, then delete them:

```sh
kubectl get pods -n zeabur-terminals \
  -o jsonpath='{range .items[?(@.metadata.annotations.zeabur\.com/tools-project=="shop")]}{.metadata.name}{"\n"}{end}'
```
