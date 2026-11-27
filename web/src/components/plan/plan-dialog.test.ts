import { describe, expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { warningsOf } from "@/components/plan/plan-dialog"
import type { Translate } from "@/i18n"

const t = ((key: string, ...args: unknown[]) =>
  args.length > 0 ? `${key}(${args.join(", ")})` : key) as unknown as Translate

describe("the warnings of a review", () => {
  test("lists what the form warned of before what the plan found", () => {
    const plan = { objects: [], warnings: [], gaps: [] } as unknown as Schemas.PlanResponse
    const said = warningsOf(t, plan, [{ tool: "Helm", name: "web" }], ["Not enough memory free"])
    expect(said.map((s) => s.text)).toStrictEqual([
      "Not enough memory free",
      "plan.toolSkipped(common.list(web), 1, Helm, plan.when(Helm))",
    ])
  })
})
