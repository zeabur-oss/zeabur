// The image sets (docs/internal/screenshots.md): each is a spec here, <set>.ts, with one test for
// each shot. `bun e2e/shots/sets.ts <set>` prints a set's settings as JSON for
// scripts/docs-shots.sh.
//   out     where the images are kept, from the repository's root; without it target/docs-shots/<set>
//   file    the path in <out>; {docs} is the locale's docs folder, empty for en, else <locale>/
//   scale   pixels for each CSS pixel
//   budget  the largest AVIF in KB
//   clean   empty each folder the run writes into, so images whose shot is gone go too
//   apps    the demo servers its shots use (fixtures.ts)
export const locales = { en: "en-US", "zh-TW": "zh-TW", "zh-CN": "zh-CN" }
export type Locale = keyof typeof locales
export type Theme = "light" | "dark"
export type App = "zeabur" | "installer"

type Set = {
  out?: string
  file: string
  scale: 1 | 2
  budget: number
  clean: boolean
  locales: Locale[]
  themes: Theme[]
  apps: App[]
}

export const sets: Record<string, Set> = {
  // The documentation and oss.zeabur.com.
  docs: {
    out: "docs",
    file: "{docs}images/shots/{id}.avif",
    scale: 2,
    budget: 150,
    clean: true,
    locales: ["en", "zh-TW", "zh-CN"],
    themes: ["light"],
    apps: ["zeabur", "installer"],
  },
  // The README: English, one pixel per CSS pixel, at most 30 KB each.
  readme: {
    out: "docs/images",
    file: "{id}-{theme}.avif",
    scale: 1,
    budget: 30,
    clean: false,
    locales: ["en"],
    themes: ["light", "dark"],
    apps: ["zeabur"],
  },
  // The plate of the README cover. It goes into design-system's covers/zeabur, which renders the
  // cover: scripts/docs-shots.sh cover <design-system>/covers/zeabur
  cover: {
    file: "{id}-{theme}.avif",
    scale: 2,
    budget: 150,
    clean: false,
    locales: ["en"],
    themes: ["light", "dark"],
    apps: ["zeabur"],
  },
}

if (import.meta.main) {
  const set = sets[process.argv[2]]
  if (!set) {
    console.error(`unknown set "${process.argv[2]}": ${Object.keys(sets).join(", ")}`)
    process.exit(1)
  }
  console.log(JSON.stringify(set))
}
