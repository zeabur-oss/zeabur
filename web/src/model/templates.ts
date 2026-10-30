// Starting points for objects written as YAML: the common kinds filled in, any other kind the
// cluster serves as its skeleton. The Deployment and the Service go together (the same labels).
import type { Schemas } from "@/api/client"

const common: Record<string, string> = {
  Deployment: `apiVersion: apps/v1
kind: Deployment
metadata:
  name: hello
spec:
  replicas: 1
  selector:
    matchLabels:
      app: hello
  template:
    metadata:
      labels:
        app: hello
    spec:
      containers:
        - name: hello
          image: nginx:1.29
          ports:
            - containerPort: 80
          resources:
            requests:
              cpu: 100m
              memory: 128Mi
            limits:
              memory: 128Mi
          readinessProbe:
            httpGet:
              path: /
              port: 80
`,
  Service: `apiVersion: v1
kind: Service
metadata:
  name: hello
spec:
  selector:
    app: hello
  ports:
    - port: 80
      targetPort: 80
`,
  ConfigMap: `apiVersion: v1
kind: ConfigMap
metadata:
  name: hello-config
data:
  LOG_LEVEL: info
`,
  Secret: `apiVersion: v1
kind: Secret
metadata:
  name: hello-env
type: Opaque
stringData:
  API_KEY: change-me
`,
  PersistentVolumeClaim: `apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: hello-data
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 1Gi
`,
  CronJob: `apiVersion: batch/v1
kind: CronJob
metadata:
  name: nightly
spec:
  schedule: "0 3 * * *"
  jobTemplate:
    spec:
      template:
        spec:
          restartPolicy: OnFailure
          containers:
            - name: nightly
              image: busybox:1.37
              command: ["sh", "-c", "date"]
              resources:
                requests:
                  cpu: 50m
                  memory: 32Mi
                limits:
                  memory: 32Mi
`,
}

/** The kinds with a filled-in template, in the order offered. */
const commonKinds = Object.keys(common)

const apiVersionOf = (k: Pick<Schemas.CreatableKind, "group" | "version">) =>
  k.group ? `${k.group}/${k.version}` : k.version

/** A kind's template: filled in for the common kinds, else its skeleton. */
export function templateOf(kind: Schemas.CreatableKind): string {
  if (!kind.group || kind.group === "apps" || kind.group === "batch") {
    const filled = common[kind.kind]
    if (filled) return filled
  }
  return `apiVersion: ${apiVersionOf(kind)}
kind: ${kind.kind}
metadata:
  name: ${kind.kind.toLowerCase()}
spec: {}
`
}

/** `existing` with `added` after it, as one more document. */
export function appended(existing: string, added: string): string {
  if (existing.trim() === "") return added
  return `${existing.replace(/\n*$/, "\n")}---\n${added}`
}

/** The kinds to offer, grouped: common kinds, custom resources, and the rest the person may create. */
export function offered(creatable: Schemas.CreatableList | undefined) {
  const items = creatable?.items ?? []
  const fixed = items.filter(
    (k) => commonKinds.includes(k.kind) && ["", "apps", "batch"].includes(k.group),
  )
  const rest = items.filter((k) => !fixed.includes(k) && k.access !== "denied")
  return {
    common: commonKinds
      .map((name) => fixed.find((k) => k.kind === name))
      .filter((k): k is Schemas.CreatableKind => k !== undefined),
    custom: rest.filter((k) => k.custom),
    more: rest.filter((k) => !k.custom),
  }
}
