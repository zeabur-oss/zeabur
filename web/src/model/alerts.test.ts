import { describe, expect, test } from "bun:test"

import { alertsFor, folded, type Alert } from "@/model/alerts"
import type { Service } from "@/model/service"

const service = (kind: Service["kind"], name: string, pods: string[] = []) =>
  ({ kind, name, pods: pods.map((p) => ({ name: p })) }) as unknown as Service

const alert = (pod?: string) => ({ rule: "Terminal shell in container", pod }) as Alert

describe("which alerts are about a service", () => {
  test("its pods now, and pods its controller named that are gone", () => {
    const web = service("Deployment", "web", ["web-7c9d8f6b5-abcde"])
    const alerts = [
      alert("web-7c9d8f6b5-abcde"),
      alert("web-5f4d3c2b1a-zyxwv"),
      alert("web-api-7c9d8f6b5-abcde"),
      alert("worker-7c9d8f6b5-abcde"),
      alert(),
    ]
    expect(alertsFor(alerts, web).map((a) => a.pod)).toEqual([
      "web-7c9d8f6b5-abcde",
      "web-5f4d3c2b1a-zyxwv",
    ])
  })

  test("each kind's pod names", () => {
    expect(alertsFor([alert("db.v2-0")], service("StatefulSet", "db.v2"))).toHaveLength(1)
    expect(alertsFor([alert("dbxv2-0")], service("StatefulSet", "db.v2"))).toHaveLength(0)
    expect(alertsFor([alert("agent-x7k2p")], service("DaemonSet", "agent"))).toHaveLength(1)
    expect(
      alertsFor([alert("nightly-29384756-q8w2e")], service("CronJob", "nightly")),
    ).toHaveLength(1)
  })
})

describe("repeats", () => {
  test("the same rule on the same container and command is one row, the newest", () => {
    const shell = (time: string, command = "sh") =>
      ({
        rule: "Terminal shell in container",
        pod: "web-1",
        container: "web",
        command,
        time,
      }) as Alert
    const rows = folded([shell("3"), shell("2", "bash"), shell("1")])
    expect(rows.map((r) => [r.alert.time, r.count])).toEqual([
      ["3", 2],
      ["2", 1],
    ])
  })
})
