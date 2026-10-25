// The project's NetworkPolicy and the ones that let another project in, as zeabur-model's `kernel`
// `isolation` writes them. https://kubernetes.io/docs/concepts/services-networking/network-policies/

const PROJECT_POLICY = "zeabur-project"
const PLATFORM_LABEL = "installer.zeabur.com/baseline"
const NAME_LABEL = "kubernetes.io/metadata.name"

export const networkPolicies = {
  group: "networking.k8s.io",
  version: "v1",
  resource: "networkpolicies",
}

const policy = (namespace: string, name: string, from: object[]) => ({
  apiVersion: "networking.k8s.io/v1",
  kind: "NetworkPolicy",
  metadata: { name, namespace, labels: { "app.kubernetes.io/managed-by": "zeabur" } },
  spec: { podSelector: {}, policyTypes: ["Ingress"], ingress: [{ from }] },
})

export const projectPolicy = (namespace: string) =>
  policy(namespace, PROJECT_POLICY, [
    { podSelector: {} },
    { namespaceSelector: { matchLabels: { [PLATFORM_LABEL]: "true" } } },
    { namespaceSelector: { matchLabels: { [NAME_LABEL]: "kube-system" } } },
  ])

/** A name is at most 63 characters and ends with a letter or digit. */
export const allowName = (from: string) =>
  `zeabur-allow-from-${from}`.slice(0, 63).replace(/-+$/, "")

/** Lets every pod of `from` reach every pod of `namespace`, on every port. */
export const allowFrom = (namespace: string, from: string) =>
  policy(namespace, allowName(from), [
    { namespaceSelector: { matchLabels: { [NAME_LABEL]: from } } },
  ])
