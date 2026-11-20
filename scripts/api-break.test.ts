import { describe, expect, test } from "bun:test";
import { breakingChanges, verdict } from "./api-break";

const document = (properties: string[]) => `openapi: "3.1.0"
info: { title: t, version: "1" }
paths:
  /api/v1/operations/{id}:
    get:
      parameters:
        - { name: id, in: path, required: true, schema: { type: string } }
      responses:
        "200":
          description: ok
          content:
            application/json:
              schema: { $ref: "#/components/schemas/OperationView" }
components:
  schemas:
    OperationView:
      type: object
      required: [${properties.join(", ")}]
      properties:
${properties.map((p) => `        ${p}: { type: string }`).join("\n")}
`;

const before = document(["id", "state"]);

describe("the API gate", () => {
  test("refuses a removed response field without a footer", async () => {
    const changes = await breakingChanges(before, document(["id"]));
    expect(changes).toContain("state");
    const result = verdict(
      changes,
      "refactor(api): drop the state\n\nSigned-off-by: a <a@b>\n",
    );
    expect(result.ok).toBe(false);
  });

  test("lets the same change pass with a footer", async () => {
    const changes = await breakingChanges(before, document(["id"]));
    const messages =
      "refactor(api)!: drop the state\n\nBREAKING CHANGE: `state` is gone.\n";
    expect(verdict(changes, messages).ok).toBe(true);
  });

  test("lets an unchanged or additive document pass", async () => {
    expect(await breakingChanges(before, before)).toBe("");
    expect(
      await breakingChanges(before, document(["id", "state", "reason"])),
    ).toBe("");
    expect(verdict("", "").ok).toBe(true);
  });

  test("does not take a footer quoted inside a line", () => {
    expect(
      verdict("change", "docs: explain the BREAKING CHANGE: footer\n").ok,
    ).toBe(false);
  });
});
