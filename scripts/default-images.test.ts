import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { readDefaultImages } from "./default-images";

const digest = "a".repeat(64);
const source = (debug: string, tools: string) =>
  `pub const DEBUG_IMAGE: &str = "${debug}";\npub const TOOLS_IMAGE: &str = "${tools}";\n`;

describe("readDefaultImages", () => {
  test("reads the debug and tools images with their labels", () => {
    expect(
      readDefaultImages(
        source(
          `docker.io/library/busybox:1.37.0@sha256:${digest}`,
          `docker.io/alpine/k8s:1.36.5@sha256:${digest}`,
        ),
      ),
    ).toEqual([
      {
        label: "debug",
        ref: `docker.io/library/busybox:1.37.0@sha256:${digest}`,
      },
      { label: "tools", ref: `docker.io/alpine/k8s:1.36.5@sha256:${digest}` },
    ]);
  });

  test("refuses an image without a digest", () => {
    expect(() =>
      readDefaultImages(
        source("docker.io/library/busybox:1.37.0", "x@sha256:" + digest),
      ),
    ).toThrow("DEBUG_IMAGE is not a reference by digest");
  });

  test("refuses a missing constant", () => {
    expect(() => readDefaultImages("")).toThrow("no DEBUG_IMAGE");
  });

  test("reads the constants the code pins", () => {
    const settings = readFileSync(
      join(import.meta.dir, "../crates/zeabur-api/src/settings.rs"),
      "utf8",
    );
    for (const { ref } of readDefaultImages(settings))
      expect(ref).toMatch(/@sha256:[0-9a-f]{64}$/);
  });
});
