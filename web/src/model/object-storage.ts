// Object storage the SeaweedFS Operator runs: a `Seaweed` with an S3 gateway, its `Bucket`s, and
// one IAM user (`S3Identity`, `S3Credentials`, `S3Policy`, `S3PolicyBinding`) that services use.
// https://github.com/seaweedfs/seaweedfs-operator/tree/seaweedfs-operator-0.1.43/api/v1
import type { Schemas } from "@/api/client"
import type { Connectable } from "@/model/connect"
import type { Event } from "@/model/service"

export type ObjectStorage = Schemas["SeaweedObjectStorage"]
export type Bucket = Schemas["SeaweedBucket"]
type Change = Schemas["Change"]

const API_VERSION = "seaweed.seaweedfs.com/v1"
const GROUP = "seaweed.seaweedfs.com"
const IMAGE = "chrislusf/seaweedfs:4.48"
const PORT = 8333

const PLURAL: Record<string, string> = {
  Seaweed: "seaweeds",
  Bucket: "buckets",
  S3Identity: "s3identities",
  S3Credentials: "s3credentials",
  S3Policy: "s3policies",
  S3PolicyBinding: "s3policybindings",
}

export interface NewObjectStorage {
  name: string
  /** The volume server's disk. */
  storage: string
  storageClass?: string
  /** S3 bucket names. */
  buckets: string[]
}

export const newObjectStorage = (): NewObjectStorage => ({
  name: "files",
  storage: "10Gi",
  buckets: [],
})

/** The IAM user Zeabur makes for services, and the name of each of its objects. */
const userOf = (store: string) => `${store}-app`

const restrictedPod = {
  runAsNonRoot: true,
  runAsUser: 1000,
  runAsGroup: 1000,
  fsGroup: 1000,
  seccompProfile: { type: "RuntimeDefault" },
}
const restricted = { allowPrivilegeEscalation: false, capabilities: { drop: ["ALL"] } }
const secured = { podSecurityContext: restrictedPod, containerSecurityContext: restricted }

const create = (object: Record<string, unknown>): Change => ({
  op: "create",
  object: object as Record<string, never>,
})

const persisted = (storageClass?: string) => ({
  enabled: true,
  resources: { requests: { storage: "1Gi" } },
  ...(storageClass ? { storageClassName: storageClass } : {}),
})

const bucketObject = (store: string, bucket: string) => ({
  apiVersion: API_VERSION,
  kind: "Bucket",
  metadata: { name: `${store}-${bucket}` },
  spec: {
    name: bucket,
    clusterRef: { name: store },
    reclaimPolicy: "Delete",
    access: [{ user: userOf(store), actions: ["Read", "Write", "List", "Tagging"] }],
  },
})

/** The objects of a new store: the `Seaweed`, its IAM user with full access, and its buckets. */
export function createObjectStorage(form: NewObjectStorage): Change[] {
  const { name, storage, storageClass } = form
  const user = userOf(name)
  const ref = { seaweedRef: { name } }
  return [
    create({
      apiVersion: API_VERSION,
      kind: "Seaweed",
      metadata: { name },
      spec: {
        image: IMAGE,
        volumeServerDiskCount: 1,
        master: { replicas: 1, persistence: persisted(storageClass), ...secured },
        volume: {
          replicas: 1,
          requests: { storage },
          ...(storageClass ? { storageClassName: storageClass } : {}),
          ...secured,
        },
        filer: { replicas: 1, persistence: persisted(storageClass), ...secured },
        s3: { replicas: 1, port: PORT, ...secured },
      },
    }),
    create({ apiVersion: API_VERSION, kind: "S3Identity", metadata: { name: user }, spec: ref }),
    create({
      apiVersion: API_VERSION,
      kind: "S3Credentials",
      metadata: { name: user },
      spec: { ...ref, identityRef: { name: user } },
    }),
    create({
      apiVersion: API_VERSION,
      kind: "S3Policy",
      metadata: { name: user },
      spec: { ...ref, statements: [{ effect: "Allow", actions: ["s3:*"], resources: ["*"] }] },
    }),
    create({
      apiVersion: API_VERSION,
      kind: "S3PolicyBinding",
      metadata: { name: user },
      spec: { ...ref, policyRef: { name: user }, subjects: [{ kind: "S3Identity", name: user }] },
    }),
    ...form.buckets.map((b) => create(bucketObject(name, b))),
  ]
}

export const createBucket = (store: string, bucket: string): Change[] => [
  create(bucketObject(store, bucket)),
]

const remove = (kind: string, name: string): Change => ({
  op: "delete",
  resource: { group: GROUP, version: "v1", resource: PLURAL[kind] ?? "" },
  name,
})

export const deleteBucket = (bucket: Bucket): Change[] => [remove("Bucket", bucket.name)]

/** Deleting the store: its buckets, its IAM objects, then the `Seaweed`. The plan also deletes the
 * volumes the operator keeps. */
export const deleteObjectStorage = (store: ObjectStorage): Change[] => [
  ...store.buckets.map((b) => remove("Bucket", b.name)),
  ...store.iam.filter((o) => PLURAL[o.kind]).map((o) => remove(o.kind, o.name)),
  remove("Seaweed", store.name),
]

export type BucketProblem = "empty" | "invalid" | "taken"

/** S3 names, kept to what is also a Kubernetes name: 3 to 63 lowercase letters, digits and
 * hyphens. */
const BUCKET = /^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$/

export function bucketProblem(bucket: string, taken: string[]): BucketProblem | undefined {
  if (bucket === "") return "empty"
  if (!BUCKET.test(bucket)) return "invalid"
  if (taken.includes(bucket)) return "taken"
  return undefined
}

/** A store as far as connecting to it goes. */
export function connectable(store: ObjectStorage): Connectable {
  const connection = store.connection ?? {
    host: `${store.name}-s3`,
    port: PORT,
    secret: "",
    passwordKey: "secretKey",
  }
  return {
    engine: "s3",
    name: store.name,
    connection,
    app: store.connection
      ? {
          username: store.connection.username ?? "",
          secret: store.connection.secret,
          passwordKey: store.connection.passwordKey,
        }
      : undefined,
    buckets: store.buckets.map((b) => b.s3Name),
  }
}

/** A store the plan is about to create, for connecting services to it. */
export function pendingObjectStorage(form: NewObjectStorage, project: string): Connectable {
  const user = userOf(form.name)
  return {
    engine: "s3",
    name: form.name,
    connection: {
      host: `${form.name}-s3.${project}.svc`,
      port: PORT,
      username: user,
      secret: user,
      usernameKey: "accessKey",
      passwordKey: "secretKey",
    },
    app: { username: user, secret: user, passwordKey: "secretKey" },
    buckets: form.buckets,
  }
}

export const storeAddress = (store: ObjectStorage) => ({
  group: GROUP,
  version: "v1",
  resource: "seaweeds",
  name: store.name,
})

/** Growing the volume servers' disks; the operator grows each claim in place. */
export function growObjectStorage(store: ObjectStorage, storage: string): Change[] {
  type Spec = { volume?: { requests?: Record<string, string> } }
  const object = structuredClone(store.object) as { spec?: Spec }
  const volume = ((object.spec ??= {}).volume ??= {})
  volume.requests = { ...volume.requests, storage: storage.trim() }
  const { name, ...resource } = storeAddress(store)
  return [{ op: "edit", resource, name, yaml: JSON.stringify(object) }]
}

/** Events about the store, its pods and its buckets. */
export function storeEvents(events: Event[], store: ObjectStorage): Event[] {
  const pods = new Set(store.pods.map((p) => p.name))
  const buckets = new Set(store.buckets.map((b) => b.name))
  return events.filter(({ regarding: r }) => {
    if (!r?.name) return false
    if (r.kind === "Seaweed") return r.name === store.name
    if (r.kind === "Bucket") return buckets.has(r.name)
    return r.kind === "Pod" && pods.has(r.name)
  })
}
