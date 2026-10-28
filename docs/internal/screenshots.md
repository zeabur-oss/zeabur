# Screenshots

Every screenshot comes from one manifest format and one script, `scripts/docs-shots.sh`, which
takes them from the demo server with Chromium from nixpkgs. Each manifest is one image set:

| Manifest                 | Images                                 | Where                                    |
| ------------------------ | -------------------------------------- | ---------------------------------------- |
| `docs/shots.yaml`        | The documentation and oss.zeabur.com   | `docs/[<locale>/]images/shots/<id>.avif` |
| `docs/readme-shots.yaml` | The README, English, 1×, at most 30 KB | `docs/images/<id>-<theme>.avif`          |
| `docs/cover-shots.yaml`  | The plate of the README cover          | `../design-system/covers/zeabur`         |

All of them are kept in git. Nothing in CI or the release takes them: run the script after
changing a page a shot covers, and commit the images.

Each locale's screenshots sit in its docs folder: `docs/images/shots` for English,
`docs/zh-TW/images/shots` for Traditional Chinese. A page shows them by a relative path, which
GitHub and the website both follow; from `docs/zh-TW/deploy/databases.mdx`:

```md
![The orders database's overview](../images/shots/database-overview.avif)
```

## Take them

```sh
nix develop -c scripts/docs-shots.sh                         # docs/shots.yaml into docs/[<locale>/]images/shots
nix develop -c scripts/docs-shots.sh docs/readme-shots.yaml  # into docs/images
nix develop -c scripts/docs-shots.sh web/e2e/shots/fixture.yaml  # into target/docs-shots
```

The script builds the web interface and the demo (`crates/zeabur-api/examples/demo.rs`), starts
one demo server for each distinct `app` and `env`, and takes each shot in every locale (`en`,
`zh-TW`, `zh-CN`) and the manifest's themes at twice the pixel density, with reduced motion. The
themes are light only unless the manifest sets `themes`. A shot can list its own `themes`, and a
shot that only one page uses can list fewer `locales`. It writes `<out>/<locale>/<theme>/<id>.avif`.
Images already in `<out>` that the run does not take stay there, unless the manifest sets
`clean`.

Each AVIF must be at most 150 KB, or the manifest's `budget`. The script lowers the AVIF quality in
steps until each file fits; it names the files that still do not and fails, and `<out>` stays as it
was. When a shot
fails, Playwright's trace is in `target/docs-shots-results`. The demo listens on port 18300;
`ZEABUR_SHOTS_PORT` picks another, for example when two worktrees take shots at once.

## Declare a shot

```yaml
- id: project-overview          # kebab-case, unique; the file name
  app: zeabur                   # optional: zeabur (default) or installer
  path: /projects/shop          # route in the demo
  wait: 'main h1'               # optional: CSS selector to wait for
  steps:                        # optional, in order
    - click: 'a[href="/projects/shop/services/Deployment/api"]'
    - fill: 'input[name="name"]'
      value: shop-2
    - hover: 'a[href="/projects/shop/settings"]'
    - press: Escape
    - wait: '[role="alert"]'
  clip: main                    # optional: CSS selector to crop to; at least 24 px around it
  until: '#cluster-nodes'       # optional, with clip: the crop ends at the bottom of this element
  blank: input                  # optional: text that changes on every run, left out of the image
  viewport: desktop             # desktop (1440×900), phone (390×844) or cover (544×544); default desktop
  locales: [en]                 # optional: en, zh-TW, zh-CN; default all three
  themes: [light]               # optional: light, dark; default the manifest's `themes`
  env:                          # optional: the demo's ZEABUR_DEMO_* variables, as strings
    ZEABUR_DEMO_FORBID: nodes,pods
  fresh: true                   # optional: a new demo for each locale and theme
```

Selectors never use visible text: it changes with the locale. Use a route in `href`, an
attribute, or a `data-testid` added to the component. A selector that matches several elements
uses the first. After the steps the pointer moves to the corner of the page, unless the last step
is `hover`.

A shot whose steps change what the demo holds, such as sending an
invitation, sets `fresh: true`: it gets its own demo server for each locale and theme, so every
locale shows the same state. The demo's `ZEABUR_DEMO_*` variables are listed at the top of
`crates/zeabur-api/examples/demo.rs`.

A shot ends where its content does. The window grows or shrinks to the height of the page, or of
`clip` when it is a scroll area such as `main`, so the image shows all of it and no empty space
below. To stop higher, at the end of a section, name that section in `until`.

With `app: installer` the script builds the installer's pages and serves them with `vite
preview` and the captured responses in `crates/zeabur-installer/web/scripts/dev-api.ts`.
`ZEABUR_DEMO_INSTALL` shows the install as `running`, `done` or `fail` at once.

The script checks the manifest before it builds anything: unknown keys, ids that are not
kebab-case or not unique, steps other than `click`, `fill`, `hover`, `press` and `wait`, an unknown `app`, and `env`
variables outside `ZEABUR_DEMO_*`.

## Same input, same pixels

Two runs give the same files, so a run changes in git only the images whose pages changed:

- The demo and the browser take `ZEABUR_DEMO_NOW` as the time now, so dates stay put.
- A value made at random, such as a new invitation's token, differs on every run: name what
  shows it in `blank`.
- A shot taken before its page settles differs too: end the steps with a `wait` for the state the
  image shows, so a run that misses it fails instead.
- Fonts render differently on macOS and Linux. Take the shots where CI runs, aarch64 Linux inside
  `nix develop` (a CI runner or a Linux VM): there, Chromium, avifenc and the fonts all come from
  the dev shell, and Chromium has only the fonts of its `FONTCONFIG_FILE`, Noto Sans CJK for
  Chinese.

## Settings

A manifest is a mapping: its settings, and the list of shots in `shots`.

```yaml
out: docs/images                # the default <out>, from the repository's root
file: '{id}-{theme}.avif'       # optional: the path in <out>; default {locale}/{theme}/{id}.avif;
                                # {docs} is the locale's docs folder, empty for en, else <locale>/
scale: 1                        # optional: pixels for each CSS pixel, 1 or 2; default 2
budget: 30                      # optional: the largest AVIF in KB; default 150
clean: true                     # optional: empty each folder the run writes into, when the set owns it
themes: [light, dark]           # optional: the themes of a shot that lists none; default [light]
shots:
  - id: readme-home
    ...
```

Without `out` the images go to `target/docs-shots`. `file` must hold `{locale}` or `{docs}` when a
shot has more than one locale, and `{theme}` when it has more than one theme.

- **Docs and site** (`docs/shots.yaml`): twice the pixel density, light only, in all three
  locales. The landing shows them at most 1152 CSS px wide, so text stays sharp on Hi-DPI screens.
  `clean` removes images whose shot is gone.
- **README** (`docs/readme-shots.yaml`): one pixel for each CSS pixel, light and dark, English only,
  at most 30 KB each. The fonts of macOS make some of them larger; take them on Linux.
- **Cover** (`docs/cover-shots.yaml`): the `cover` viewport is the plate of the README cover; the
  window keeps that size. It writes into the design-system checkout beside this one; render the
  cover there afterwards (its `covers/README.md`).
