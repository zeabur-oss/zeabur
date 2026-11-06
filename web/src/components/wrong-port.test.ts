import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { offerAsk, wrongPortWords } from "@/components/wrong-port"
import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import type { Service } from "@/model/service"
import type { WrongPort } from "@/model/wrong-port"

const checkout = { kind: "Deployment", name: "checkout" } as Service
const offer = { container: "checkout", from: 8080, to: 3000 }
const wrong: WrongPort = {
  diagnosis: { kind: "probeFailure" } as Schemas.Diagnosis,
  port: 8080,
  offer,
}

describe("the card of an app that doesn't listen on its port", () => {
  test.each([
    ["en", en, "port 8080", "0.0.0.0"],
    ["zh-TW", zhTW, "連接埠 8080", "0.0.0.0"],
    ["zh-CN", zhCN, "端口 8080", "0.0.0.0"],
  ] as const)("names the port, PORT and the all-addresses bind in %s", (_, messages, port, all) => {
    const words = wrongPortWords(translator(messages), wrong)
    expect(words.cause).toContain(port)
    expect(words.why).toContain("PORT")
    expect(words.fix).toContain("PORT")
    expect(words.fix).toContain("127.0.0.1")
    expect(words.fix).toContain(all)
  })
})

describe("the offer to use the image's port", () => {
  test("plans one change that moves the service from its port to the image's", () => {
    const ask = offerAsk(translator(en), checkout, offer)
    expect(ask.title).toBe("Use port 3000 for checkout")
    expect(ask.changes).toStrictEqual([
      {
        op: "port",
        workload: { kind: "Deployment", name: "checkout" },
        container: "checkout",
        from: 8080,
        to: 3000,
      },
    ])
  })
})
