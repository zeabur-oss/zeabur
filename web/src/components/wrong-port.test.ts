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
    ["en", en, "port 8080", "port 3000"],
    ["zh-TW", zhTW, "連接埠 8080", "連接埠是 3000"],
    ["zh-CN", zhCN, "端口 8080", "端口是 3000"],
  ] as const)(
    "names the port, PORT, the bind and the image's port in %s",
    (_, messages, port, image) => {
      const t = translator(messages)
      const words = wrongPortWords(t, wrong)
      expect(words.cause).toContain(port)
      expect(words.why).toContain("PORT")
      for (const fix of [words.fix, wrongPortWords(t, { ...wrong, offer: undefined }).fix]) {
        expect(fix).toContain("PORT")
        expect(fix).toContain("127.0.0.1")
        expect(fix).toContain("0.0.0.0")
      }
      expect(words.fix).toContain(image)
    },
  )
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
