// Adding a worker node: the bootstrap token Secret a plan creates, the token string and the
// commands the new VM runs, the way `zeabur-installer node add` prints them.
import type { Schemas } from "@/api/client"

type JoinInfo = Schemas["JoinInfo"]
type NodeView = Schemas["NodeView"]

/** Minutes a token works for: long enough to join a VM, short enough not to linger. */
export const LIFETIMES = [30, 60] as const
export type Lifetime = (typeof LIFETIMES)[number]
export const DEFAULT_LIFETIME: Lifetime = 60

const ALPHABET = "abcdefghijklmnopqrstuvwxyz0123456789"
export const AUTH_GROUP = "system:bootstrappers:rke2:default-node-token"
const DESCRIPTION = "Zeabur: add node"

/** `length` letters and digits from `random`, without the bias of a plain modulo. */
export function randomText(
  length: number,
  random: (bytes: Uint8Array<ArrayBuffer>) => void,
): string {
  const limit = 256 - (256 % ALPHABET.length)
  let out = ""
  while (out.length < length) {
    const bytes = new Uint8Array(length * 2)
    random(bytes)
    for (const byte of bytes) {
      if (byte < limit && out.length < length) out += ALPHABET[byte % ALPHABET.length]
    }
  }
  return out
}

export interface JoinToken {
  id: string
  secret: string
}

export function newToken(
  taken: readonly string[] = [],
  random: (bytes: Uint8Array<ArrayBuffer>) => void = (b) => crypto.getRandomValues(b),
): JoinToken {
  let id = randomText(6, random)
  while (taken.includes(id)) id = randomText(6, random)
  return { id, secret: randomText(16, random) }
}

export const secretName = (id: string) => `bootstrap-token-${id}`

/** RFC 3339 in UTC, to the second. */
export const expiration = (now: Date, minutes: number) =>
  new Date(now.getTime() + minutes * 60_000).toISOString().replace(/\.\d{3}Z$/, "Z")

/** The Secret `rke2 token create` writes, labelled as Zeabur's. */
export function tokenSecret(token: JoinToken, expires: string) {
  return {
    apiVersion: "v1",
    kind: "Secret",
    type: "bootstrap.kubernetes.io/token",
    metadata: {
      name: secretName(token.id),
      namespace: "kube-system",
      labels: { "app.kubernetes.io/managed-by": "zeabur" },
    },
    stringData: {
      "token-id": token.id,
      "token-secret": token.secret,
      description: DESCRIPTION,
      expiration: expires,
      "usage-bootstrap-authentication": "true",
      "usage-bootstrap-signing": "true",
      "auth-extra-groups": AUTH_GROUP,
    },
  }
}

export const tokenString = (caHash: string, token: JoinToken) =>
  `K10${caHash}::${token.id}.${token.secret}`

/** `https://<host>:<port>`, nothing else; IPv6 in brackets. */
export function serverProblem(server: string): "empty" | "invalid" | undefined {
  const text = server.trim()
  if (text === "") return "empty"
  if (!/^https:\/\/(\[[0-9a-fA-F:.]+\]|[A-Za-z0-9.-]+):\d{1,5}$/.test(text)) return "invalid"
  const port = Number(text.slice(text.lastIndexOf(":") + 1))
  return port > 0 && port < 65536 ? undefined : "invalid"
}

/** The block pasted on the new VM as root: the join configuration, then plan and apply. */
export function joinScript({
  server,
  token,
  expect,
}: {
  server: string
  token: string
  expect?: Schemas["JoinExpect"]
}): string {
  const config = [
    "join:",
    `  server: ${server.trim()}`,
    "  role: worker",
    `  token: ${token}`,
    ...(expect
      ? [
          "  expect:",
          `    provider: ${quoted(expect.provider)}`,
          `    region: ${quoted(expect.region)}`,
        ]
      : []),
  ]
  return [
    "install -d -m 0700 /etc/zeabur-installer",
    "(umask 077; cat > /etc/zeabur-installer/config.yaml) <<'EOF'",
    ...config,
    "EOF",
    "zeabur-installer plan",
    "zeabur-installer apply",
    "",
  ].join("\n")
}

const quoted = (value: string) => (/^[A-Za-z0-9._-]+$/.test(value) ? value : JSON.stringify(value))

/** Why Zeabur cannot add a node here, if it cannot. */
export type Unavailable = "notRke2" | "noCaHash"

export function unavailable(info: JoinInfo): Unavailable | undefined {
  if (info.blocked === "notRke2") return "notRke2"
  if (!info.caHash) return "noCaHash"
  return undefined
}

/** A node that was not there when the token was made: the first one, Ready ones first. */
export function joined(
  before: ReadonlySet<string>,
  nodes: readonly NodeView[],
): { name: string; ready: boolean } | undefined {
  const fresh = nodes
    .filter((n) => !before.has(n.name))
    .map((n) => ({ name: n.name, ready: n.ready === "True" }))
    .toSorted((a, b) => Number(b.ready) - Number(a.ready) || a.name.localeCompare(b.name))
  return fresh[0]
}

/** Milliseconds until `expires`; `undefined` for a token that never expires. */
const timeLeft = (expires: string | null | undefined, now: number) =>
  expires == null ? undefined : Date.parse(expires) - now

/** The tokens still live at `now`, soonest to expire first (the API's order). */
export const liveTokens = (tokens: readonly Schemas["JoinToken"][], now: number) =>
  tokens.filter((t) => {
    const left = timeLeft(t.expires, now)
    return left === undefined || left > 0
  })
