// The images Zeabur runs by default without publishing them, read from the constants the code
// pins (crates/zeabur-api/src/settings.rs), so digests.txt and the code share one source.
export const defaultImages = [
  { label: "debug", constant: "DEBUG_IMAGE" },
  { label: "tools", constant: "TOOLS_IMAGE" },
];

export type DefaultImage = { label: string; ref: string };

export function readDefaultImages(source: string): DefaultImage[] {
  return defaultImages.map(({ label, constant }) => {
    const found = new RegExp(`pub const ${constant}: &str = "([^"]+)";`).exec(
      source,
    );
    if (!found) throw new Error(`settings.rs has no ${constant}`);
    const ref = found[1]!;
    if (!/^[^\s@]+@sha256:[0-9a-f]{64}$/.test(ref))
      throw new Error(`${constant} is not a reference by digest: ${ref}`);
    return { label, ref };
  });
}
