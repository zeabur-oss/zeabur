import { describe, test, expect } from "bun:test"

import { roleYaml, subjectOf } from "@/model/rbac"

const rules = [{ apiGroup: "", resources: ["pods"], verbs: ["get"] }]

describe("the role to ask for", () => {
  test("binds a user by name", () => {
    expect(subjectOf("alice@example.com")).toStrictEqual({
      kind: "User",
      name: "alice@example.com",
    })
    const yaml = roleYaml({ name: "r", rules, username: "alice@example.com" })
    expect(yaml).toMatch(
      /subjects:\n {2}- kind: User\n {4}apiGroup: rbac\.authorization\.k8s\.io\n {4}name: "alice@example\.com"\n$/,
    )
    expect(yaml).toMatch(/kind: ClusterRoleBinding\nmetadata:\n {2}name: r-alice-example\.com\n/)
  })

  test("binds a service account login as the ServiceAccount, in its namespace", () => {
    expect(subjectOf("system:serviceaccount:shop:deployer")).toStrictEqual({
      kind: "ServiceAccount",
      namespace: "shop",
      name: "deployer",
    })
    const yaml = roleYaml({
      name: "r",
      namespace: "web",
      rules,
      username: "system:serviceaccount:shop:deployer",
    })
    expect(yaml).toMatch(
      /subjects:\n {2}- kind: ServiceAccount\n {4}name: "deployer"\n {4}namespace: "shop"\n$/,
    )
    expect(yaml).not.toMatch(/kind: User/)
    // The Role and its binding sit in the namespace they grant, not the account's.
    expect(yaml).toMatch(/kind: Role\nmetadata:\n {2}name: r\n {2}namespace: web\n/)
    expect(yaml).toMatch(
      /kind: RoleBinding\nmetadata:\n {2}name: r-system-serviceaccount-shop-deployer\n {2}namespace: web\n/,
    )
  })

  test("a name that only looks like a service account is a user", () => {
    expect(subjectOf("system:serviceaccount:shop").kind).toBe("User")
    expect(subjectOf("system:serviceaccount:a:b:c").kind).toBe("User")
  })

  test("quotes rules and opens with the warning as comments", () => {
    const yaml = roleYaml({
      name: "r",
      rules: [{ apiGroup: "", resources: ["namespaces"], resourceNames: ["shop"], verbs: ["get"] }],
      username: "",
      warning: "reads every Secret",
    })
    expect(yaml).toMatch(/^# reads every Secret\n#\n/)
    expect(yaml).toMatch(
      /- apiGroups: \[""\]\n {4}resources: \["namespaces"\]\n {4}resourceNames: \["shop"\]\n {4}verbs: \["get"\]/,
    )
    expect(yaml).toMatch(/name: r-user\n/)
  })
})
