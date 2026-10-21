import { expect, test } from "bun:test"

import { createdKey, entrance, markCreated } from "@/lib/created"

const web = { kind: "Deployment" as const, name: "web" }

test("only what a change just added enters, and only for a while", () => {
  markCreated(
    [
      { op: "env", workload: web, changes: [{ name: "PORT", op: "set", value: "80" }] },
      { op: "env", workload: web, changes: [{ name: "OLD", op: "remove" }] },
      { op: "addDomain", workload: web, hostname: "a.example.com" },
    ],
    1000,
  )
  expect(entrance(createdKey.env(web, "PORT"), 2000)).toBe("swap-in")
  expect(entrance(createdKey.domain("a.example.com"), 2000)).toBe("swap-in")
  expect(entrance(createdKey.env(web, "OLD"), 2000)).toBeUndefined()
  expect(entrance(createdKey.env(web, "PORT"), 20_000)).toBeUndefined()
})
