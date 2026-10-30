import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { portNote, shownPort, tcpPorts } from "@/model/image-ports"

const read = (...ports: [number, string][]): Schemas.ImageInspection => ({
  image: "nginx",
  platform: "linux/amd64",
  exposedPorts: ports.map(([port, protocol]) => ({ port, protocol })),
  user: "",
  runsAsRoot: true,
})

const nginx = read([80, "TCP"])

describe("the port field", () => {
  test("takes the image's first TCP port until the person types one", () => {
    expect(shownPort(undefined, nginx)).toBe("80")
    expect(shownPort(undefined, read([53, "UDP"], [8080, "TCP"], [9090, "TCP"]))).toBe("8080")
    expect(shownPort("3000", nginx)).toBe("3000")
    expect(shownPort("", nginx)).toBe("")
  })

  test("stays empty while the image is unread, unreadable or declares nothing", () => {
    expect(shownPort(undefined, undefined)).toBe("")
    expect(shownPort(undefined, read())).toBe("")
    expect(shownPort(undefined, { image: "x", exposedPorts: [], unreadable: "unreachable" })).toBe(
      "",
    )
  })

  test("leaves out UDP and SCTP ports", () => {
    expect(tcpPorts(read([53, "UDP"], [5000, "SCTP"], [443, "TCP"]))).toEqual([443])
  })
})

describe("the note under it", () => {
  test("says where one port came from and offers the choice among several", () => {
    expect(portNote("80", nginx)).toEqual({ kind: "read", port: 80 })
    expect(portNote("80", read([80, "TCP"], [443, "TCP"]))).toEqual({
      kind: "choose",
      ports: [80, 443],
    })
  })

  test("suggests the image's ports when the person typed another, without changing it", () => {
    expect(portNote("8080", nginx)).toEqual({ kind: "differs", ports: [80] })
    expect(portNote("", nginx)).toEqual({ kind: "differs", ports: [80] })
  })

  test("says why the image could not be read, and nothing before it is", () => {
    expect(portNote("", undefined)).toBeUndefined()
    expect(portNote("", read())).toBeUndefined()
    expect(
      portNote("", {
        image: "x",
        exposedPorts: [],
        unreadable: "no-platform",
        detail: "linux/s390x",
      }),
    ).toEqual({ kind: "unreadable", reason: "no-platform", detail: "linux/s390x" })
  })
})
