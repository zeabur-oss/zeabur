# Screenshots

Every screenshot is a Playwright test in `web/e2e/shots`, taken from the demo server with Chromium
from nixpkgs by `scripts/docs-shots.sh`. Each spec is one image set, with its settings in
`web/e2e/shots/sets.ts`:

| Set      | Spec                      | Images                                 | Where                                    |
| -------- | ------------------------- | -------------------------------------- | ---------------------------------------- |
| `docs`   | `web/e2e/shots/docs.ts`   | The documentation and oss.zeabur.com   | `docs/[<locale>/]images/shots/<id>.avif` |
| `readme` | `web/e2e/shots/readme.ts` | The README, English, 1×, at most 30 KB | `docs/images/<id>-<theme>.avif`          |
| `cover`  | `web/e2e/shots/cover.ts`  | The plate of the README cover          | design-system's `covers/zeabur`          |

All of them are kept in git. Run the script after changing a page a shot covers, and commit the
images. When the release pull request's branch, `release/next`, gets its release commit,
`.github/workflows/shots.yml` takes the `docs` set again, so the images show the version being
released, and opens a pull request into `release/next`, `update/shots`, with the images that
changed ([Publish a release](releasing.md#publish-a-release)).

Each locale's screenshots sit in its docs folder: `docs/images/shots` for English,
`docs/zh-TW/images/shots` for Traditional Chinese. A page shows them by a relative path, which
GitHub and the website both follow; from `docs/zh-TW/deploy/databases.mdx`:

```md
![The orders database's overview](../images/shots/database-overview.avif)
```

## Take them

```sh
nix develop -c scripts/docs-shots.sh                  # docs into docs/[<locale>/]images/shots
nix develop -c scripts/docs-shots.sh readme           # into docs/images
nix develop -c scripts/docs-shots.sh cover ../design-system/covers/zeabur
nix develop -c scripts/docs-shots.sh docs target/try -- -g installer   # some of them, elsewhere
```

The script builds the web interface and the demo (`crates/zeabur-api/examples/demo.rs`), runs the
set's spec in every locale and theme of the set at its pixel density, with reduced motion, and
encodes each image as AVIF into `<out>`: the second argument, else the set's `out`, else
`target/docs-shots/<set>`. Arguments after `--` go to Playwright. Images already in `<out>` that the
run does not take stay there, unless the set sets `clean` and the run takes all of them.

Each AVIF must be at most the set's `budget`. The script lowers the AVIF quality in steps until each
file fits; it names the files that still do not and fails, and `<out>` stays as it was. When a shot
fails, Playwright's trace is in `target/docs-shots-results`.

## Write a shot

A shot is a test titled by its id, kebab-case and unique, which is the file name. It opens a page,
does what the image needs, and ends with `shot()`:

```ts
import { expect, test } from "./fixtures.ts"

test("cluster-nodes", async ({ page, shot }) => {
  await page.goto("/cluster", { waitUntil: "networkidle" })
  await expect(page.locator("main h1")).toBeVisible()
  await shot({ clip: "main", until: "#cluster-nodes" })
})
```

`shot()` takes:

- `clip`: a CSS selector to crop to, with at least 24 px around it.
- `until`: with `clip`, the crop ends at the bottom of this element.
- `blank`: a selector whose text changes on every run, left out of the image.
- `keepPointer`: leave the pointer where the steps left it, to show a hover state. Otherwise it
  moves to the corner of the page first.
- `fit: false`: keep the window at its size, for the cover's plate.

Selectors never use visible text: it changes with the locale. Use a route in `href`, an attribute,
or a `data-testid` added to the component.

The demo a shot runs on is set with `test.use`, in a `test.describe` around the shots that share it:

```ts
test.describe(() => {
  test.use({ app: "installer", demo: { ZEABUR_DEMO_INSTALL: "done" } })
  // …
})
```

- `app`: `zeabur` (default) or `installer`.
- `demo`: the demo's `ZEABUR_DEMO_*` variables, listed at the top of
  `crates/zeabur-api/examples/demo.rs`. The shots with the same `app` and `demo` share a server in
  each Playwright worker.
- `fresh: true`: for a shot whose steps change what the demo holds, such as sending an invitation.
  It gets its own demo server for each locale and theme, so every locale shows the same state.

A shot ends where its content does. The window grows or shrinks to the height of the page, or of
`clip` when it is a scroll area such as `main`, so the image shows all of it and no empty space
below. To stop higher, at the end of a section, name that section in `until`.

With `app: "installer"` the shots use the installer's pages, served by `vite preview` with the
captured responses in `crates/zeabur-installer/web/scripts/dev-api.ts`. `ZEABUR_DEMO_INSTALL` shows
the install as `running`, `done` or `fail` at once.

## Same input, same pixels

Two runs give the same files, so a run changes in git only the images whose pages changed:

- The demo and the browser take `ZEABUR_DEMO_NOW` as the time now, so dates stay put.
- A value made at random, such as a new invitation's token, differs on every run: name what
  shows it in `blank`.
- A shot taken before its page settles differs too: end the steps with an `expect` for the state
  the image shows, so a run that misses it fails instead.
- Fonts render differently on macOS and Linux. Take the shots where CI runs, aarch64 Linux inside
  `nix develop` (a CI runner or a Linux VM): there, Chromium, avifenc and the fonts all come from
  the dev shell, and Chromium has only the fonts of its `FONTCONFIG_FILE`, Noto Sans CJK for
  Chinese.

## Sets

A set in `web/e2e/shots/sets.ts` has `out`, where its images are kept from the repository's root;
`file`, the path in `<out>`, with `{id}`, `{locale}`, `{docs}` (the locale's docs folder, empty for
`en`, else `<locale>/`) and `{theme}`; `scale`, 1 or 2 pixels for each CSS pixel; `budget`, the
largest AVIF in KB; `clean`; its `locales` and `themes`; and the `apps` its shots use, which the
script builds. The Playwright config makes a project for each set, locale and theme, such as
`docs/zh-TW/light`.

- **Docs and site** (`docs`): twice the pixel density, light only, in all three locales. The
  landing shows them at most 1152 CSS px wide, so text stays sharp on Hi-DPI screens. `clean`
  removes images whose shot is gone.
- **README** (`readme`): one pixel for each CSS pixel, light and dark, English only, at most 30 KB
  each. The fonts of macOS make some of them larger; take them on Linux.
- **Cover** (`cover`): the plate of the README cover, at 544×544. It has no `out`: pass
  design-system's `covers/zeabur`, then render the cover there (its `covers/README.md`).
