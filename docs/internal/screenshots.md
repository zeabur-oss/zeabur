# Screenshots

Every screenshot comes from one manifest format and one script, `scripts/docs-shots.sh`, which
takes them from the demo server with Chromium from nixpkgs. Each manifest is one image set:

| Manifest                 | Images                                 | Where                                         |
| ------------------------ | -------------------------------------- | --------------------------------------------- |
| `docs/shots.yaml`        | The documentation, each locale         | `docs-shots-<version>.tar` of each release    |
| `docs/readme-shots.yaml` | The README, English, 1×, at most 30 KB | `docs/images/<id>-<theme>.avif`               |
| `docs/site-shots.yaml`   | The landing page at oss.zeabur.com     | `docs/images/site/<locale>/<theme>/<id>.avif` |
| `docs/cover-shots.yaml`  | The plate of the README cover          | `../design-system/covers/zeabur`              |

The documentation's screenshots are not in git: the website downloads the tar for the version it
shows. The others are.

## Take them

```sh
nix develop -c scripts/docs-shots.sh                         # docs/shots.yaml into target/docs-shots
nix develop -c scripts/docs-shots.sh docs/readme-shots.yaml  # into docs/images
nix develop -c scripts/docs-shots.sh web/e2e/shots/fixture.yaml target/docs-shots v0.1.0
```

The script builds the web interface and the demo (`crates/zeabur-api/examples/demo.rs`), starts
one demo server for each distinct `app` and `env`, and takes each shot in every locale (`en`,
`zh-TW`, `zh-CN`) and the manifest's themes at twice the pixel density, with reduced motion. The
themes are light only unless the manifest sets `themes`; `docs/shots.yaml` takes light and dark,
and the site shows the one the reader's theme is in. A shot can list its own `themes`, and a shot
that only one page uses can list fewer `locales`. It writes:

```text
<out>/<locale>/<theme>/<id>.avif
<out>/manifest.json               every file and its size in bytes
<out>/docs-shots-<version>.tar    both, when a version is given
```

Images already in `<out>` that the run does not take stay there, and `manifest.json` lists them
too, so a run with a shorter manifest replaces only its own images.

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

Two runs on the same machine on the same day give the same files. Two limits:

- The demo dates its objects relative to the moment it starts, so a page that shows a date
  (for example "Created on Sep 26, 2026") changes from one day to the next.
- Fonts render differently on macOS and Linux. The release takes the shots on Linux. There,
  Chromium has only the fonts of the dev shell's `FONTCONFIG_FILE`: Noto Sans CJK for Chinese.

## Image sets kept in git

A manifest for images kept in git is a mapping: its settings, and the list of shots in `shots`.

```yaml
out: docs/images                # the default <out>, from the repository's root
file: '{id}-{theme}.avif'       # optional: the path in <out>; default {locale}/{theme}/{id}.avif
scale: 1                        # optional: pixels for each CSS pixel, 1 or 2; default 2
budget: 30                      # optional: the largest AVIF in KB; default 150
clean: true                     # optional: empty <out> first, when the set owns it
themes: [light, dark]           # optional: the themes of a shot that lists none; default [light]
shots:
  - id: readme-home
    ...
```

With `out`, the run writes only the images: no `manifest.json` and no tar. `file` must hold
`{locale}` or `{theme}` when a shot has more than one of them. `docs/shots.yaml` is a mapping too,
for its `themes`, but names no `out`.

- **README** (`docs/readme-shots.yaml`): one pixel for each CSS pixel, light and dark, English only,
  at most 30 KB each. The fonts of macOS make some of them larger; take them on Linux.
- **Landing page** (`docs/site-shots.yaml`): twice the pixel density, light and dark, in all three
  locales. The landing shows them at most 1152 CSS px wide, so text stays sharp on Hi-DPI screens.
  `clean` removes images whose shot is gone.
- **Cover** (`docs/cover-shots.yaml`): the `cover` viewport is the plate of the README cover; the
  window keeps that size. It writes into the design-system checkout beside this one; render the
  cover there afterwards (its `covers/README.md`).
