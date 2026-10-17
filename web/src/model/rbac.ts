// The RBAC to ask an administrator for, as YAML ready to apply: a Role and RoleBinding in one
// namespace, or a ClusterRole and ClusterRoleBinding, bound to the person who is signed in.

/** One rule of a Role (RBAC `PolicyRule`). */
export interface PolicyRule {
  apiGroup: string
  resources: string[]
  resourceNames?: string[]
  verbs: string[]
}

/** Who a binding names: a service account for a `system:serviceaccount:<namespace>:<name>`
 * username, which is how kube-apiserver names one, and a user for any other
 * (https://kubernetes.io/docs/reference/access-authn-authz/rbac/#referring-to-subjects). Groups
 * are not offered: binding a group grants the role to everyone in it, not only to the person who
 * asks. */
export type Subject =
  | { kind: "User"; name: string }
  | { kind: "ServiceAccount"; namespace: string; name: string }

export function subjectOf(username: string): Subject {
  const account = username.match(/^system:serviceaccount:([^:]+):([^:]+)$/)
  return account?.[1] && account[2]
    ? { kind: "ServiceAccount", namespace: account[1], name: account[2] }
    : { kind: "User", name: username }
}

const quote = (value: string) => JSON.stringify(value)
const list = (values: string[]) => `[${values.map(quote).join(", ")}]`

/** `username` reduced to the characters of an object name, for the binding's name: two people who
 * ask for the same role get two bindings, not one that the second overwrites. */
function nameSuffix(username: string): string {
  return username
    .toLowerCase()
    .replace(/[^a-z0-9.-]+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "")
    .slice(0, 200)
}

function subjectYaml(subject: Subject): string[] {
  switch (subject.kind) {
    case "User":
      return [
        "  - kind: User",
        "    apiGroup: rbac.authorization.k8s.io",
        `    name: ${quote(subject.name)}`,
      ]
    case "ServiceAccount":
      return [
        "  - kind: ServiceAccount",
        `    name: ${quote(subject.name)}`,
        `    namespace: ${quote(subject.namespace)}`,
      ]
  }
}

/** A role named `name` with `rules` and its binding to `username`: in `namespace` when given (a
 * Role), else cluster-wide (a ClusterRole). `warning`, when given, opens the YAML as comments, so
 * it reaches the administrator who applies it. */
export function roleYaml({
  name,
  namespace,
  rules,
  username,
  warning,
}: {
  name: string
  namespace?: string
  rules: PolicyRule[]
  username: string
  warning?: string
}): string {
  const kind = namespace === undefined ? "ClusterRole" : "Role"
  const metadata = (objectName: string) => [
    "metadata:",
    `  name: ${objectName}`,
    ...(namespace === undefined ? [] : [`  namespace: ${namespace}`]),
  ]
  return [
    ...(warning ? [`# ${warning}`, "#"] : []),
    "apiVersion: rbac.authorization.k8s.io/v1",
    `kind: ${kind}`,
    ...metadata(name),
    "rules:",
    ...rules.flatMap((rule) => [
      `  - apiGroups: ${list([rule.apiGroup])}`,
      `    resources: ${list(rule.resources)}`,
      ...(rule.resourceNames ? [`    resourceNames: ${list(rule.resourceNames)}`] : []),
      `    verbs: ${list(rule.verbs)}`,
    ]),
    "---",
    "apiVersion: rbac.authorization.k8s.io/v1",
    `kind: ${kind}Binding`,
    ...metadata(`${name}-${nameSuffix(username) || "user"}`),
    "roleRef:",
    "  apiGroup: rbac.authorization.k8s.io",
    `  kind: ${kind}`,
    `  name: ${name}`,
    "subjects:",
    ...subjectYaml(subjectOf(username)),
    "",
  ].join("\n")
}
