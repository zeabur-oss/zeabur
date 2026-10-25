import { describe, test, expect } from "bun:test"

import { localProblem, type Problem } from "@/api/client"
import { translator } from "@/i18n"
import { en } from "@/i18n/en"
import { zhCN } from "@/i18n/zh-CN"
import { zhTW } from "@/i18n/zh-TW"
import { problemText, refusedField } from "@/model/problem"

const denied = { verb: "list", group: "", resource: "nodes" }

function refused(overrides: Partial<Problem> = {}): Problem {
  return {
    type: "/problems/forbidden",
    title: "You do not have permission",
    status: 403,
    detail: "You do not have permission to read the cluster's nodes.",
    fix: "Ask a cluster administrator for access, or use an account that has it.",
    kind: "forbidden",
    summary: {
      key: "problem.summary.forbidden",
      params: { verb: "list", resource: "nodes" },
    },
    denied,
    ...overrides,
  }
}

const quota: Problem = {
  type: "/problems/quota-exceeded",
  title: "The project's limit does not allow this",
  status: 403,
  detail: 'deployments.apps "web" is forbidden: exceeded quota: compute',
  fix: "Ask for less, free some of what the project uses, or ask a cluster administrator to raise the limit compute.",
  kind: "quota-exceeded",
  summary: { key: "problem.summary.quota-exceeded", params: { name: "web" } },
  why: {
    key: "problem.why.quota-exceeded",
    params: { quota: "compute", resource: "requests.cpu", hard: "4", used: "3500m" },
  },
  fixKey: { key: "problem.quotaExceeded.fix.scale", params: { quota: "compute", object: "web" } },
}

describe("problem text", () => {
  test("what happened, why and what to do are the keyed sentences, in every language", () => {
    for (const locale of ["en", "zh-TW", "zh-CN"] as const) {
      const messages = { en, "zh-TW": zhTW, "zh-CN": zhCN }[locale]
      const text = problemText(translator(messages), locale, quota)
      expect(text.headline).toBe(
        messages["sentence.problem.summary.quota-exceeded"]({ name: "web" }),
      )
      expect(text.why).toBe(
        messages["sentence.problem.why.quota-exceeded"](quota.why?.params ?? {}),
      )
      expect(text.fix).toBe(
        messages["sentence.problem.quotaExceeded.fix.scale"](quota.fixKey?.params ?? {}),
      )
      // The English detail says what the summary says: it is folded with the raw answer.
      expect(text.folded).toStrictEqual([quota.detail])
    }
  })

  test("without a keyed fix, the kind's fix comes from the catalogue", () => {
    const text = problemText(translator(zhTW), "zh-TW", refused())
    expect(text.headline).toBe("你不能列出 nodes。")
    expect(text.fix).toBe(zhTW["problem.fix.forbidden"])
    // The refusal is said by the summary; the server's English sentence is folded away.
    expect(text.why).toBe(undefined)
    expect(text.folded).toStrictEqual(["You do not have permission to read the cluster's nodes."])
  })

  test("a fix written for the occasion without a key is shown in English, folded otherwise", () => {
    const fix = "Ask a cluster administrator for `list` on `nodes` (cluster-wide)."
    expect(problemText(translator(en), "en", refused({ fix })).fix).toBe(fix)
    const zh = problemText(translator(zhTW), "zh-TW", refused({ fix }))
    expect(zh.fix).toBe(zhTW["problem.fix.forbidden"])
    expect(zh.folded).toStrictEqual([
      "You do not have permission to read the cluster's nodes.",
      fix,
    ])
  })

  test("a detail that is the only explanation is the why, marked as English", () => {
    const invalid = problemText(translator(zhTW), "zh-TW", {
      ...refused(),
      type: "/problems/invalid",
      kind: "invalid",
      status: 422,
      detail: "spec.replicas: must be greater than or equal to 0",
      fix: en["problem.fix.invalid"],
      summary: { key: "problem.summary.invalid", params: { name: "web" } },
      denied: undefined,
    })
    expect(invalid.why).toBe("spec.replicas: must be greater than or equal to 0")
    expect(invalid.whyInEnglish).toBe(true)
    expect(invalid.folded).toStrictEqual([])
    // A refusal without `denied` has nothing else that says what was refused.
    const bare = problemText(translator(zhTW), "zh-TW", refused({ denied: undefined }))
    expect(bare.why).toBe("You do not have permission to read the cluster's nodes.")
  })

  test("a policy's own words are quoted, not translated", () => {
    const text = problemText(translator(zhTW), "zh-TW", {
      ...quota,
      type: "/problems/policy-denied",
      kind: "policy-denied",
      summary: { key: "problem.summary.policy-denied", params: { name: "web" } },
      why: {
        key: "problem.why.policy-denied",
        params: { policy: "require-labels", message: "label team is required" },
      },
      fixKey: { key: "problem.policyDenied.fix.change" },
    })
    expect(text.why).toBe("政策 require-labels 的回應是：")
    expect(text.quote).toBe("label team is required")
  })

  test("a refused field marks the form's field it is, is in, or holds", () => {
    const fields = [
      { field: "spec.replicas", message: "must be greater than or equal to 0" },
      { field: "spec.template.spec.containers[0].resources.requests", reason: "FieldValueInvalid" },
    ]
    expect(refusedField(fields, "spec.replicas")).toBe("must be greater than or equal to 0")
    const cpu = "spec.template.spec.containers[0].resources.requests.cpu"
    expect(refusedField(fields, cpu)).toBe("FieldValueInvalid")
    expect(refusedField(fields, "spec.template.spec.containers[1].resources.requests.cpu")).toBe(
      undefined,
    )
    // A name that only begins the same is another field.
    expect(refusedField([{ field: "spec.replicasMax" }], "spec.replicas")).toBe(undefined)
  })

  test("a problem made on this page is written from its keys; a newer kind is shown as it came", () => {
    const local = localProblem(0, "Failed to fetch")
    const written = problemText(translator(zhTW), "zh-TW", local)
    expect(written.headline).toBe(zhTW["sentence.page.noAnswer"]())
    expect(written.fix).toBe(zhTW["sentence.page.reload"]())
    expect(written.why).toBe("Failed to fetch")
    const newer = {
      ...local,
      type: "/problems/new",
      kind: "new",
      title: "No answer",
      summary: { key: "problem.summary.new" },
    } as unknown as Problem
    expect(problemText(translator(zhTW), "zh-TW", newer).headline).toBe("No answer")
    const text = problemText(translator(zhTW), "zh-TW", newer)
    expect(text.why).toBe("Failed to fetch")
    expect(text.whyInEnglish).toBe(true)
  })
})
