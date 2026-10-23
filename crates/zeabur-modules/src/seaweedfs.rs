//! The `SeaweedFS` Operator: S3-compatible object storage from `Seaweed` objects, with buckets and
//! access keys as objects of their own [sw-api].
//!
//! A `Seaweed` runs a master, volume servers, a filer and an S3 gateway. The operator labels
//! each pod with `app.kubernetes.io/name: seaweedfs` and `app.kubernetes.io/instance: <name>`.
//! The gateway is the Service `<name>-s3`, on port 8333 unless `spec.s3.port` says otherwise. The
//! status counts the replicas of each part and sets the `Ready` condition [sw-status].
//!
//! Access goes through the gateway's embedded IAM. An `S3Identity` is a user; an `S3Credentials`
//! gives it an access key, which the operator writes to a Secret (`accessKey` and `secretKey`,
//! named after the `S3Credentials`). A `Bucket` grants actions on itself to users in
//! `spec.access`, and keeps its contents when the object goes unless `spec.reclaimPolicy` is
//! `Delete`.
//!
//! [sw-api]: https://github.com/seaweedfs/seaweedfs-operator/tree/seaweedfs-operator-0.1.43/api/v1
//! [sw-status]: https://github.com/seaweedfs/seaweedfs-operator/blob/seaweedfs-operator-0.1.43/internal/controller/seaweed_controller.go

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;
use zeabur_kube::DynamicObject;

use crate::databases::{self, Connection, State};
use crate::{Module, Read, text};

/// The module.
pub static MODULE: Module = Module {
    id: "seaweedfs-operator",
    capability: "object-storage",
    reads: &[SEAWEEDS, BUCKETS, CREDENTIALS],
    http: &[],
};

const API_VERSION: &str = "seaweed.seaweedfs.com/v1";

/// `seaweed.seaweedfs.com/v1` `Seaweed`, in the project's namespace.
pub const SEAWEEDS: Read = Read {
    group: "seaweed.seaweedfs.com",
    version: "v1",
    resource: "seaweeds",
    kind: "Seaweed",
};

/// `seaweed.seaweedfs.com/v1` `Bucket`, in the project's namespace.
pub const BUCKETS: Read = Read {
    group: "seaweed.seaweedfs.com",
    version: "v1",
    resource: "buckets",
    kind: "Bucket",
};

/// `seaweed.seaweedfs.com/v1` `S3Credentials`, in the project's namespace.
pub const CREDENTIALS: Read = Read {
    group: "seaweed.seaweedfs.com",
    version: "v1",
    resource: "s3credentials",
    kind: "S3Credentials",
};

/// The parts of a `Seaweed`, as its spec and status name them.
const PARTS: [&str; 4] = ["master", "volume", "filer", "s3"];

/// One bucket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SeaweedBucket)]
pub struct Bucket {
    /// The `Bucket` object.
    pub origin: zeabur_model::Origin,
    /// The object's name.
    pub name: String,
    /// The S3 bucket name.
    pub bucket: String,
    /// `Pending`, `Ready`, `Failed` or `Terminating`, as the operator says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// The message of a condition that is not true, when there is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The bytes stored, as last counted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub used_bytes: Option<i64>,
    /// The objects stored, as last counted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objects: Option<i64>,
    /// Deleting the object deletes the bucket and its contents.
    pub deletes_contents: bool,
}

/// The access key services use, from an `S3Credentials` of the store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SeaweedAccessKey)]
pub struct AccessKey {
    /// The `S3Credentials` object's name.
    pub name: String,
    /// The IAM user it belongs to.
    pub user: String,
    /// The access key ID, once the operator made it; it is not secret.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_key: Option<String>,
    /// `Pending`, `Ready`, `Failed` or `Terminating`, as the operator says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
}

/// One object store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(as = SeaweedObjectStorage)]
pub struct ObjectStorage {
    /// The `Seaweed` object.
    pub origin: zeabur_model::Origin,
    /// Its name.
    pub name: String,
    /// How it is.
    pub state: State,
    /// The operator's message, when it is not ready.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Pods asked for, over the master, volume servers, filer and S3 gateway.
    pub members: i64,
    /// Pods ready.
    pub ready: i64,
    /// The `SeaweedFS` version, from the image tag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The `SeaweedFS` image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// The size of each volume server's disk, as asked for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage: Option<String>,
    /// The `StorageClass` of the volume servers' disks; absent for the cluster's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub storage_class: Option<String>,
    /// Its PersistentVolumeClaims, as the operator names them. The operator keeps them when the
    /// store is deleted.
    pub claims: Vec<String>,
    /// Where to connect, inside the cluster, and the Secret with the access key. Absent until the
    /// store has an access key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<Connection>,
    /// The access key services use; absent when the store has none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_key: Option<AccessKey>,
    /// Its buckets, by name.
    pub buckets: Vec<Bucket>,
    /// When it was created, RFC 3339.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// The object as written, without `status` and `managedFields`.
    #[schema(value_type = Object)]
    pub object: Value,
    /// Its pods, by name; empty when they cannot be read.
    pub pods: Vec<zeabur_model::PodSummary>,
}

impl ObjectStorage {
    /// Whether a pod with `labels` is one of its pods.
    #[must_use]
    pub fn is_member(&self, labels: &std::collections::BTreeMap<String, String>) -> bool {
        labels.get("app.kubernetes.io/instance") == Some(&self.name)
            && labels.get("app.kubernetes.io/name").map(String::as_str) == Some("seaweedfs")
    }
}

/// The name of the `Seaweed` an object points at in the same namespace.
fn store_of(object: &DynamicObject, field: &str) -> Option<String> {
    let namespace = text(object, &format!("/spec/{field}/namespace"));
    if namespace.is_some() && namespace != object.metadata.namespace {
        return None;
    }
    text(object, &format!("/spec/{field}/name"))
}

/// The message of the first condition that is not true.
fn trouble(object: &DynamicObject) -> Option<String> {
    object
        .data
        .pointer("/status/conditions")?
        .as_array()?
        .iter()
        .find(|c| c["status"] != "True")
        .and_then(|c| c["message"].as_str())
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
}

fn bucket(namespace: &str, object: &DynamicObject) -> Bucket {
    let name = object.metadata.name.clone().unwrap_or_default();
    Bucket {
        origin: zeabur_model::Origin::complete(API_VERSION, "Bucket", Some(namespace), &name),
        bucket: text(object, "/status/bucketName")
            .or_else(|| text(object, "/spec/name"))
            .unwrap_or_else(|| name.clone()),
        phase: text(object, "/status/phase"),
        message: trouble(object),
        used_bytes: databases::int(object, "/status/usage/sizeBytes"),
        objects: databases::int(object, "/status/usage/objectCount"),
        deletes_contents: text(object, "/spec/reclaimPolicy").as_deref() == Some("Delete"),
        name,
    }
}

/// The access key of a store: the `S3Credentials` named `<store>-app`, as Zeabur makes it, else the
/// first by name.
fn access_key<'a>(store: &str, credentials: &'a [DynamicObject]) -> Option<&'a DynamicObject> {
    let own = format!("{store}-app");
    let mut mine: Vec<&DynamicObject> = credentials
        .iter()
        .filter(|c| store_of(c, "seaweedRef").as_deref() == Some(store))
        .collect();
    mine.sort_by_key(|c| {
        (
            c.metadata.name.as_deref() != Some(&own),
            c.metadata.name.clone(),
        )
    });
    mine.first().copied()
}

/// The pods asked for and ready, over the parts the spec runs.
fn counts(seaweed: &DynamicObject) -> (i64, i64) {
    PARTS
        .iter()
        .filter(|part| seaweed.data.pointer(&format!("/spec/{part}")).is_some())
        .fold((0, 0), |(members, ready), part| {
            let asked = databases::int(seaweed, &format!("/status/{part}/replicas"))
                .or_else(|| databases::int(seaweed, &format!("/spec/{part}/replicas")))
                .unwrap_or(1);
            let up = databases::int(seaweed, &format!("/status/{part}/readyReplicas")).unwrap_or(0);
            (members + asked, ready + up)
        })
}

/// The claims of the volume servers' disks, and of the master and filer when they persist.
fn claims(seaweed: &DynamicObject, name: &str) -> Vec<String> {
    let replicas =
        |part: &str| databases::int(seaweed, &format!("/spec/{part}/replicas")).unwrap_or(1);
    let disks = databases::int(seaweed, "/spec/volumeServerDiskCount").unwrap_or(1);
    let mut out = Vec::new();
    if seaweed.data.pointer("/spec/volume").is_some() {
        for i in 0..replicas("volume").max(0) {
            out.extend((0..disks.max(0)).map(|d| format!("mount{d}-{name}-volume-{i}")));
        }
    }
    for part in ["master", "filer"] {
        let persists = seaweed
            .data
            .pointer(&format!("/spec/{part}/persistence/enabled"))
            .and_then(Value::as_bool)
            == Some(true)
            && seaweed
                .data
                .pointer(&format!("/spec/{part}/persistence/existingClaim"))
                .is_none();
        if persists {
            out.extend(
                (0..replicas(part).max(0)).map(|i| format!("{name}-{part}-{name}-{part}-{i}")),
            );
        }
    }
    out
}

fn state(seaweed: &DynamicObject, members: i64, ready: i64) -> State {
    let condition = seaweed
        .data
        .pointer("/status/conditions")
        .and_then(Value::as_array)
        .and_then(|all| all.iter().find(|c| c["type"] == "Ready"))
        .and_then(|c| c["status"].as_str());
    match condition {
        Some("True") => databases::by_members(members, ready),
        Some(_) if ready > 0 => State::Degraded,
        Some(_) => State::Progressing,
        None => State::Unknown,
    }
}

/// One `Seaweed` as an object store, with its buckets and the access key services use.
#[must_use]
pub fn object_storage(
    namespace: &str,
    seaweed: &DynamicObject,
    buckets: &[DynamicObject],
    credentials: &[DynamicObject],
) -> ObjectStorage {
    let name = seaweed.metadata.name.clone().unwrap_or_default();
    let (members, ready) = counts(seaweed);
    let state = state(seaweed, members, ready);
    let image = text(seaweed, "/spec/image");
    let port = databases::int(seaweed, "/spec/s3/port")
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(8333);
    let key = access_key(&name, credentials);
    let secret = key.map(|k| {
        (
            text(k, "/spec/secretRef/name")
                .or_else(|| k.metadata.name.clone())
                .unwrap_or_default(),
            text(k, "/spec/secretRef/accessKeyField").unwrap_or_else(|| "accessKey".to_owned()),
            text(k, "/spec/secretRef/secretKeyField").unwrap_or_else(|| "secretKey".to_owned()),
            text(k, "/status/identityName").or_else(|| text(k, "/spec/identityRef/name")),
        )
    });
    let mut buckets: Vec<Bucket> = buckets
        .iter()
        .filter(|b| store_of(b, "clusterRef").as_deref() == Some(name.as_str()))
        .map(|b| bucket(namespace, b))
        .collect();
    buckets.sort_by(|a, b| a.bucket.cmp(&b.bucket));
    ObjectStorage {
        origin: zeabur_model::Origin::complete(API_VERSION, "Seaweed", Some(namespace), &name),
        state,
        message: (state != State::Ready)
            .then(|| databases::condition_message(seaweed, "Ready"))
            .flatten(),
        members,
        ready,
        version: text(seaweed, "/spec/version")
            .or_else(|| image.as_deref().and_then(databases::tag).map(str::to_owned)),
        image,
        storage: text(seaweed, "/spec/volume/requests/storage"),
        storage_class: text(seaweed, "/spec/volume/storageClassName"),
        claims: claims(seaweed, &name),
        connection: secret
            .as_ref()
            .map(|(secret, access, private, user)| Connection {
                host: format!("{name}-s3.{namespace}.svc"),
                port,
                database: None,
                username: user.clone(),
                secret: secret.clone(),
                username_key: Some(access.clone()),
                password_key: private.clone(),
                options: None,
            }),
        access_key: key.map(|k| AccessKey {
            name: k.metadata.name.clone().unwrap_or_default(),
            user: secret
                .as_ref()
                .and_then(|s| s.3.clone())
                .unwrap_or_default(),
            access_key: text(k, "/status/accessKey"),
            phase: text(k, "/status/phase"),
        }),
        buckets,
        created: seaweed
            .metadata
            .creation_timestamp
            .as_ref()
            .map(|t| t.0.to_string()),
        object: databases::written(seaweed),
        pods: Vec::new(),
        name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(value: Value) -> DynamicObject {
        serde_json::from_value(value).unwrap()
    }

    fn seaweed(status: Value) -> DynamicObject {
        object(serde_json::json!({
            "apiVersion": API_VERSION, "kind": "Seaweed",
            "metadata": {"name": "files", "namespace": "shop"},
            "spec": {
                "image": "chrislusf/seaweedfs:4.48",
                "master": {"replicas": 1, "persistence": {"enabled": true}},
                "filer": {"replicas": 1, "persistence": {"enabled": true}},
                "s3": {"replicas": 1},
                "volume": {"replicas": 1, "requests": {"storage": "10Gi"},
                    "storageClassName": "longhorn"}
            },
            "status": status
        }))
    }

    #[test]
    fn a_ready_store_with_its_key_and_buckets() {
        let credentials = [
            object(serde_json::json!({
                "apiVersion": API_VERSION, "kind": "S3Credentials",
                "metadata": {"name": "other", "namespace": "shop"},
                "spec": {"seaweedRef": {"name": "elsewhere"}, "identityRef": {"name": "x"}}
            })),
            object(serde_json::json!({
                "apiVersion": API_VERSION, "kind": "S3Credentials",
                "metadata": {"name": "files-app", "namespace": "shop"},
                "spec": {"seaweedRef": {"name": "files"}, "identityRef": {"name": "files-app"}},
                "status": {"accessKey": "AKIA1", "phase": "Ready", "identityName": "files-app"}
            })),
        ];
        let buckets = [
            object(serde_json::json!({
                "apiVersion": API_VERSION, "kind": "Bucket",
                "metadata": {"name": "uploads", "namespace": "shop"},
                "spec": {"clusterRef": {"name": "files"}, "reclaimPolicy": "Delete"},
                "status": {"phase": "Ready", "bucketName": "uploads",
                    "usage": {"sizeBytes": 2048, "objectCount": 3}}
            })),
            object(serde_json::json!({
                "apiVersion": API_VERSION, "kind": "Bucket",
                "metadata": {"name": "theirs", "namespace": "shop"},
                "spec": {"clusterRef": {"name": "files", "namespace": "other"}}
            })),
        ];
        let store = object_storage(
            "shop",
            &seaweed(serde_json::json!({
                "conditions": [{"type": "Ready", "status": "True", "message": "ok"}],
                "master": {"replicas": 1, "readyReplicas": 1},
                "volume": {"replicas": 1, "readyReplicas": 1},
                "filer": {"replicas": 1, "readyReplicas": 1},
                "s3": {"replicas": 1, "readyReplicas": 1}
            })),
            &buckets,
            &credentials,
        );
        assert_eq!(
            (store.state, store.members, store.ready),
            (State::Ready, 4, 4)
        );
        assert_eq!(store.version.as_deref(), Some("4.48"));
        assert_eq!(store.message, None);
        let connection = store.connection.unwrap();
        assert_eq!(
            (connection.host.as_str(), connection.port),
            ("files-s3.shop.svc", 8333)
        );
        assert_eq!(connection.secret, "files-app");
        assert_eq!(connection.username_key.as_deref(), Some("accessKey"));
        assert_eq!(connection.password_key, "secretKey");
        assert_eq!(
            store.access_key.unwrap().access_key.as_deref(),
            Some("AKIA1")
        );
        assert_eq!(store.buckets.len(), 1);
        let uploads = &store.buckets[0];
        assert!(uploads.deletes_contents);
        assert_eq!((uploads.used_bytes, uploads.objects), (Some(2048), Some(3)));
    }

    #[test]
    fn a_new_store_is_progressing_and_has_no_connection_without_a_key() {
        let store = object_storage(
            "shop",
            &seaweed(serde_json::json!({
                "conditions": [{"type": "Ready", "status": "False",
                    "message": "Master: 0/1 ready, Volume: 0/1 ready"}]
            })),
            &[],
            &[],
        );
        assert_eq!((store.state, store.ready), (State::Progressing, 0));
        assert_eq!(
            store.message.as_deref(),
            Some("Master: 0/1 ready, Volume: 0/1 ready")
        );
        assert!(store.connection.is_none());
        assert_eq!(store.storage_class.as_deref(), Some("longhorn"));
        assert_eq!(
            store.claims,
            [
                "mount0-files-volume-0",
                "files-master-files-master-0",
                "files-filer-files-filer-0"
            ]
        );
    }
}
