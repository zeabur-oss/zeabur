import { describe, expect, test } from "bun:test"

import { yamlLanguage } from "@codemirror/lang-yaml"

import { toYaml } from "@/lib/yaml-out"

describe("toYaml", () => {
  test("writes nested maps and lists in block style", () => {
    expect(
      toYaml([
        {
          kind: "Deployment",
          metadata: { name: "web", labels: { "app.kubernetes.io/name": "web" } },
          spec: { containers: [{ name: "web", ports: [{ containerPort: 80 }] }], empty: [] },
        },
      ]),
    ).toBe(
      [
        "kind: Deployment",
        "metadata:",
        "  name: web",
        "  labels:",
        "    app.kubernetes.io/name: web",
        "spec:",
        "  containers:",
        "  - name: web",
        "    ports:",
        "    - containerPort: 80",
        "  empty: []",
        "",
      ].join("\n"),
    )
  })

  test("quotes strings YAML would read as something else", () => {
    const text = toYaml([{ a: "true", b: "8080", c: "", d: "x: y", e: "#tag", f: "ok value" }])
    expect(text).toBe('a: "true"\nb: "8080"\nc: ""\nd: "x: y"\ne: "#tag"\nf: ok value\n')
  })

  test("separates documents and parses", () => {
    const text = toYaml([{ a: 1 }, { b: [1, "two"] }])
    expect(text).toBe("a: 1\n---\nb:\n- 1\n- two\n")
    let errors = 0
    yamlLanguage.parser.parse(text).iterate({
      enter(node) {
        if (node.type.isError) errors += 1
      },
    })
    expect(errors).toBe(0)
  })
})
