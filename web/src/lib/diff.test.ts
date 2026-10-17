import { describe, test, expect } from "bun:test"

import { diffLines, hunks, type DiffLine } from "@/lib/diff"
import { forAll, type Random } from "@/test/property"

// The plan dialog draws the change between a planned object's before and after with diffLines and
// hunks: what the person approves. A line dropped from the diff or a change folded away would
// misstate the write.

/** A text of a few short lines, from few enough words that two texts share some. */
const text = (random: Random) =>
  random.list(8, () => random.pick(["a", "b", "c", "key: 1", "", "  - x"])).join("\n")

const side = (diff: DiffLine[], kind: "add" | "remove") =>
  diff
    .filter((line) => line.kind !== kind)
    .map((line) => line.text)
    .join("\n")

describe("diffLines", () => {
  test("its unchanged and removed lines are the old text, its unchanged and added the new", () => {
    forAll((random) => {
      const before = text(random)
      const after = text(random)
      const diff = diffLines(before, after)
      expect(side(diff, "add")).toBe(before)
      expect(side(diff, "remove")).toBe(after)
    })
  })

  test("equal texts differ in nothing; a changed line is removed and added", () => {
    forAll((random) => {
      const same = text(random)
      expect(diffLines(same, same).every((line) => line.kind === "same")).toBeTruthy()
    })
    expect(diffLines("a\nb\nc", "a\nB\nc")).toStrictEqual([
      { kind: "same", text: "a" },
      { kind: "remove", text: "b" },
      { kind: "add", text: "B" },
      { kind: "same", text: "c" },
    ])
    expect(diffLines("", "a")).toStrictEqual([{ kind: "add", text: "a" }])
  })
})

describe("hunks", () => {
  test("keep every changed line, in order, with at most `context` unchanged lines around", () => {
    forAll((random) => {
      const diff = diffLines(text(random), text(random))
      const context = random.int(0, 3)
      const shown = hunks(diff, context)
      const kept = shown.filter((line): line is DiffLine => line !== null)
      // In the diff's order, and every changed line among them.
      const at = kept.map((line) => diff.indexOf(line))
      expect(at.every((index, i) => index > (at[i - 1] ?? -1))).toBeTruthy()
      expect(kept.filter((line) => line.kind !== "same")).toStrictEqual(
        diff.filter((line) => line.kind !== "same"),
      )
      // An unchanged line is shown only near a change.
      const changed = diff.flatMap((line, index) => (line.kind === "same" ? [] : [index]))
      for (const index of at)
        expect(changed.some((c) => Math.abs(c - index) <= context)).toBeTruthy()
      // A gap marks only skipped lines: never first, last or twice in a row.
      expect(shown[0]).not.toBe(null)
      expect(shown.at(-1)).not.toBe(null)
      expect(shown.every((line, i) => line !== null || shown[i + 1] !== null)).toBeTruthy()
      for (let i = 1; i < shown.length; i++)
        if (shown[i - 1] && shown[i])
          expect(diff.indexOf(shown[i]!)).toBe(diff.indexOf(shown[i - 1]!) + 1)
    })
  })
})
