import { describe, expect, test } from "bun:test"

import {
  changesOf,
  editedCount,
  fromDotenv,
  parseDotenv,
  pastedLines,
  pasteInto,
  problemsOf,
  quote,
  toDotenv,
  type EnvRow,
} from "@/model/env-edit"

const row = (name: string, value: string, over: Partial<EnvRow> = {}): EnvRow => ({
  id: name,
  container: "app",
  name,
  value,
  was: { name, value },
  removed: false,
  ...over,
})

describe("parseDotenv", () => {
  test("reads names, quotes, export, comments and blank lines", () => {
    const text = [
      "# database",
      "",
      "export DB_HOST=db.shop.svc",
      "PORT = 5432 # the default",
      `GREETING="hello \\"you\\"\\nbye"`,
      "RAW='a # b'",
      "EMPTY=",
      "not a line",
      `MULTI="one`,
      `two"`,
    ].join("\n")
    expect(parseDotenv(text)).toEqual({
      entries: [
        { name: "DB_HOST", value: "db.shop.svc", line: 3 },
        { name: "PORT", value: "5432", line: 4 },
        { name: "GREETING", value: 'hello "you"\nbye', line: 5 },
        { name: "RAW", value: "a # b", line: 6 },
        { name: "EMPTY", value: "", line: 7 },
        { name: "MULTI", value: "one\ntwo", line: 9 },
      ],
      unread: [8],
    })
  })

  test("reads back what it writes", () => {
    const values = ["plain", "with space", 'qu"ote', "back\\slash", "new\nline", "", "#hash"]
    const rows = values.map((value, i) => row(`V${i}`, value))
    expect(parseDotenv(toDotenv(rows)).entries.map((e) => e.value)).toEqual(values)
    expect(quote("plain")).toBe("plain")
  })

  test("an unclosed quote is a line it cannot read", () => {
    expect(parseDotenv(`A="open\nB=1`).unread).toEqual([1])
  })
})

describe("changesOf", () => {
  test("one change per container, removals first, a rename as both", () => {
    const rows = [
      row("KEEP", "1"),
      row("EDIT", "new", { was: { name: "EDIT", value: "old" } }),
      row("GONE", "x", { removed: true }),
      row("NEW_NAME", "v", { was: { name: "OLD_NAME", value: "v" } }),
      { id: "n1", container: "app", name: "ADDED", value: "a", removed: false },
      { id: "n2", container: "app", name: "", value: "", removed: false },
      { id: "n3", container: "sidecar", name: "SIDE", value: "s", removed: false },
    ]
    expect(changesOf(rows)).toEqual([
      {
        container: "app",
        changes: [
          { name: "GONE", op: "remove" },
          { name: "OLD_NAME", op: "remove" },
          { name: "EDIT", op: "set", value: "new" },
          { name: "NEW_NAME", op: "set", value: "v" },
          { name: "ADDED", op: "set", value: "a" },
        ],
      },
      { container: "sidecar", changes: [{ name: "SIDE", op: "set", value: "s" }] },
    ])
    expect(editedCount(rows)).toBe(5)
  })

  test("a name removed and set again is only set", () => {
    const rows = [
      row("A", "1", { removed: true }),
      { id: "n", container: "app", name: "A", value: "2", removed: false },
    ]
    expect(changesOf(rows)).toEqual([
      { container: "app", changes: [{ name: "A", op: "set", value: "2" }] },
    ])
  })
})

describe("problemsOf", () => {
  test("flags empty, invalid, repeated and taken names, not removed or blank rows", () => {
    const rows: EnvRow[] = [
      row("OK", "1"),
      { id: "empty", container: "app", name: " ", value: "v", removed: false },
      { id: "bad", container: "app", name: "1ABC", value: "v", removed: false },
      { id: "dup1", container: "app", name: "TWICE", value: "a", removed: false },
      { id: "dup2", container: "app", name: "TWICE", value: "b", removed: false },
      { id: "taken", container: "app", name: "DB_PASSWORD", value: "x", removed: false },
      row("1GONE", "x", { removed: true }),
      { id: "blank", container: "app", name: "", value: "", removed: false },
    ]
    const problems = problemsOf(rows, new Map([["app", new Set(["DB_PASSWORD"])]]))
    expect(Object.fromEntries(problems)).toEqual({
      empty: "empty",
      bad: "invalid",
      dup1: "duplicate",
      dup2: "duplicate",
      taken: "taken",
    })
  })
})

describe("text and paste", () => {
  test("text keeps the rows it names, removes the others and adds the new", () => {
    const rows = [row("A", "1"), row("B", "2"), row("C", "3", { container: "other" })]
    const next = fromDotenv(rows, "app", parseDotenv("B=20\nD=4").entries)
    expect(next.map((r) => [r.name, r.value, r.removed, r.was?.name])).toEqual([
      ["C", "3", false, "C"],
      ["A", "1", true, "A"],
      ["B", "20", false, "B"],
      ["D", "4", false, undefined],
    ])
  })

  test("pasted lines fill the row, update a named one and add the rest after it", () => {
    const rows: EnvRow[] = [
      row("A", "1"),
      { id: "n", container: "app", name: "", value: "", removed: false },
      { id: "m", container: "app", name: "LAST", value: "", removed: false },
    ]
    const pasted = pastedLines("X=1\nA=10\nY=2")
    expect(pasted).toBeDefined()
    let id = 0
    const next = pasteInto(rows, "n", pasted ?? [], () => `p${id++}`)
    expect(next.map((r) => [r.id, r.name, r.value])).toEqual([
      ["A", "A", "10"],
      ["n", "X", "1"],
      ["p0", "Y", "2"],
      ["m", "LAST", ""],
    ])
    expect(pastedLines("JUST_A_NAME")).toBeUndefined()
  })
})
