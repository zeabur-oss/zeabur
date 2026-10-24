import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import {
  AUTH_GROUP,
  expiration,
  joined,
  joinScript,
  liveTokens,
  policiesState,
  serverHost,
  newToken,
  randomText,
  serverProblem,
  tokenSecret,
  tokenString,
  unavailable,
} from "@/model/join"

const counting = () => {
  let next = 0
  return (bytes: Uint8Array) => {
    for (let i = 0; i < bytes.length; i++) bytes[i] = next++ % 256
  }
}

const node = (name: string, ready: string) => ({ name, ready }) as Schemas["NodeView"]

describe("join tokens", () => {
  test("ids and secrets are lowercase letters and digits of the contract's lengths", () => {
    const token = newToken()
    expect(token.id).toMatch(/^[a-z0-9]{6}$/)
    expect(token.secret).toMatch(/^[a-z0-9]{16}$/)
  })

  test("bytes past the last whole alphabet are skipped, not folded in", () => {
    const high = (bytes: Uint8Array) => bytes.fill(255)
    let calls = 0
    const text = randomText(3, (bytes) => {
      calls++
      if (calls === 1) high(bytes)
      else counting()(bytes)
    })
    expect(text).toBe("abc")
  })

  test("a taken id is drawn again", () => {
    const random = counting()
    const first = newToken([], counting())
    expect(newToken([first.id], random).id).not.toBe(first.id)
  })

  test("the Secret is what rke2 token create writes", () => {
    const secret = tokenSecret({ id: "abc123", secret: "0123456789abcdef" }, "2026-10-24T08:00:00Z")
    expect(secret).toMatchObject({
      type: "bootstrap.kubernetes.io/token",
      metadata: {
        name: "bootstrap-token-abc123",
        namespace: "kube-system",
        labels: { "app.kubernetes.io/managed-by": "zeabur" },
      },
      stringData: {
        "token-id": "abc123",
        "token-secret": "0123456789abcdef",
        expiration: "2026-10-24T08:00:00Z",
        "usage-bootstrap-authentication": "true",
        "usage-bootstrap-signing": "true",
        "auth-extra-groups": AUTH_GROUP,
      },
    })
    expect(AUTH_GROUP).toBe("system:bootstrappers:rke2:default-node-token")
  })

  test("expiry is RFC 3339 UTC to the second", () => {
    expect(expiration(new Date("2026-10-23T08:00:00.123Z"), 60)).toBe("2026-10-23T09:00:00Z")
    expect(expiration(new Date("2026-10-23T08:00:00Z"), 30)).toBe("2026-10-23T08:30:00Z")
  })

  test("the token string pins the CA", () => {
    const hash = "a".repeat(64)
    expect(tokenString(hash, { id: "abc123", secret: "0123456789abcdef" })).toBe(
      `K10${hash}::abc123.0123456789abcdef`,
    )
  })

  test("only unexpired tokens are listed", () => {
    const now = Date.parse("2026-10-23T08:00:00Z")
    const tokens = [
      { id: "a", secret: "bootstrap-token-a", expires: "2026-10-23T07:00:00Z" },
      { id: "b", secret: "bootstrap-token-b", expires: "2026-10-23T09:00:00Z" },
      { id: "c", secret: "bootstrap-token-c" },
    ]
    expect(liveTokens(tokens, now).map((t) => t.id)).toEqual(["b", "c"])
  })
})

describe("the commands on the new VM", () => {
  test("write the join configuration 0600, then plan and apply", () => {
    expect(
      joinScript({
        server: " https://10.0.0.5:9345 ",
        token: "K10abc::id.secret",
        expect: { provider: "hetzner", region: "fsn1" },
      }),
    ).toBe(
      [
        "install -d -m 0700 /etc/zeabur-installer",
        "(umask 077; cat > /etc/zeabur-installer/config.yaml) <<'EOF'",
        "join:",
        "  server: https://10.0.0.5:9345",
        "  role: worker",
        "  token: K10abc::id.secret",
        "  expect:",
        "    provider: hetzner",
        "    region: fsn1",
        "EOF",
        "zeabur-installer plan     # what it installs, network checks and warnings; changes nothing",
        "zeabur-installer apply    # installs RKE2 as an agent and waits for the node to be Ready",
        "",
      ].join("\n"),
    )
  })

  test("expect is left out without a location, and odd values are quoted", () => {
    const plain = joinScript({ server: "https://a:9345", token: "t" })
    expect(plain).not.toContain("expect")
    const odd = joinScript({
      server: "https://a:9345",
      token: "t",
      expect: { provider: "aws", region: "us east" },
    })
    expect(odd).toContain('region: "us east"')
  })

  test("the server is an https URL with a port", () => {
    expect(serverProblem("https://10.0.0.5:9345")).toBeUndefined()
    expect(serverProblem("https://[fd00::5]:9345")).toBeUndefined()
    expect(serverProblem("https://server.internal:9345")).toBeUndefined()
    expect(serverProblem("")).toBe("empty")
    expect(serverProblem("10.0.0.5:9345")).toBe("invalid")
    expect(serverProblem("https://10.0.0.5")).toBe("invalid")
    expect(serverProblem("https://10.0.0.5:99999")).toBe("invalid")
    expect(serverProblem("https://10.0.0.5:9345/x")).toBe("invalid")
  })
})

describe("when a node can be added and when it has joined", () => {
  const info = {
    kubernetesVersion: "v1.34.1+rke2r1",
    caHash: "a".repeat(64),
    tokens: [],
    unreadable: [],
  } satisfies Schemas["JoinInfo"]

  test("only RKE2 with a CA hash", () => {
    expect(unavailable(info)).toBeUndefined()
    expect(unavailable({ ...info, blocked: "notRke2", caHash: undefined })).toBe("notRke2")
    expect(unavailable({ ...info, caHash: undefined })).toBe("noCaHash")
  })

  test("a new node is found, a Ready one first", () => {
    const before = new Set(["server-1"])
    expect(joined(before, [node("server-1", "True")])).toBeUndefined()
    expect(joined(before, [node("server-1", "True"), node("vm-2", "False")])).toEqual({
      name: "vm-2",
      ready: false,
    })
    expect(
      joined(before, [node("vm-2", "Unknown"), node("vm-3", "True"), node("server-1", "True")]),
    ).toEqual({ name: "vm-3", ready: true })
  })
})

describe("network policies after a node joins", () => {
  const policies = {
    addresses: ["10.0.0.5", "10.0.0.9"],
    policies: [{ namespace: "zeabur", missing: ["10.0.0.9"], current: false }],
    behind: [{ name: "vps-4", addresses: ["10.0.0.9"] }],
    canUpdate: "allowed",
    unreadable: [],
  } satisfies Schemas["NodePolicies"]

  test("behind while a node's address is missing, stale while a removed one's stays", () => {
    expect(policiesState(policies)).toBe("behind")
    const exact = [{ namespace: "zeabur", missing: [], current: true }]
    expect(policiesState({ ...policies, policies: exact, behind: [] })).toBe("current")
    const extra = [{ namespace: "zeabur", missing: [], current: false }]
    expect(policiesState({ ...policies, policies: extra, behind: [] })).toBe("stale")
    expect(policiesState({ ...policies, policies: [], behind: [] })).toBe("none")
    expect(
      policiesState({
        ...policies,
        unreadable: [
          {
            resource: "networkpolicies.networking.k8s.io",
            kubernetes: { code: 403, reason: "Forbidden", message: "" },
          },
        ],
      }),
    ).toBe("unreadable")
  })

  test("the server's host, for the plan's messages", () => {
    expect(serverHost("https://10.0.0.5:9345")).toBe("10.0.0.5")
    expect(serverHost(" https://[fd00::5]:9345 ")).toBe("fd00::5")
    expect(serverHost("https://server.internal:9345")).toBe("server.internal")
  })
})
