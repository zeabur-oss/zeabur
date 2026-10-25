# Documentation screenshots

The repository holds no screenshots. `docs/shots.yaml` declares them, `scripts/docs-shots.sh`
takes them from the demo server, and each release attaches them as `docs-shots-<version>.tar`.
The website downloads that file for the version it shows.

## Take them

```sh
nix develop -c scripts/docs-shots.sh                     # docs/shots.yaml into target/docs-shots
nix develop -c scripts/docs-shots.sh web/e2e/shots/fixture.yaml target/docs-shots v0.0.2
```

The script builds the web interface and the demo (`crates/zeabur-api/examples/demo.rs`), starts
one demo server for each distinct `app` and `env`, and takes each shot in every locale (`en`, `zh-TW`, `zh-CN`) and the light theme at twice the pixel
density, with reduced motion. A shot that a dark-theme `<picture>` needs lists `themes`; a shot
that only one page uses can list fewer `locales`. It writes:

```text
<out>/<locale>/<theme>/<id>.avif
<out>/manifest.json               every file and its size in bytes
<out>/docs-shots-<version>.tar    both, when a version is given
```

Images already in `<out>` that the run does not take stay there, and `manifest.json` lists them
too, so a run with a shorter manifest replaces only its own images.

Each AVIF must be at most 150 KB; the script names the files above it and fails. When a shot
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
  viewport: desktop             # desktop (1440×900) or phone (390×844); default desktop
  locales: [en]                 # optional: en, zh-TW, zh-CN; default all three
  themes: [light, dark]         # optional: light, dark; default [light]
  env:                          # optional: the demo's ZEABUR_DEMO_* variables, as strings
    ZEABUR_DEMO_FORBID: nodes,pods
  fresh: true                   # optional: a new demo for each locale and theme
```

Selectors never use visible text: it changes with the locale. Use a route in `href`, an
attribute, or a `data-testid` added to the component. A selector that matches several elements
uses the first. After the steps the pointer moves to the corner of the page, unless the last step
is `hover`.

A shot whose steps change what the demo holds, such as creating a join token or sending an
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
- Fonts render differently on macOS and Linux. The release takes the shots on Linux.

## README images

The README's screenshots are the exception: they are in git, in `docs/images`, at most 30 KB
each. `docs/readme-shots.yaml` declares them, and `scripts/readme-images.sh` takes them at one
pixel per CSS pixel, light and dark, English only. The script sets `ZEABUR_SHOTS_SCALE=1` and
`ZEABUR_SHOTS_BUDGET_KB=30` for `scripts/docs-shots.sh`, which lowers the AVIF quality in steps
until each file fits.

## Landing images

The landing page at oss.zeabur.com uses its own set, also in git: `docs/site-shots.yaml` declares
them and `scripts/site-images.sh` takes them at twice the pixel density, light and dark, in all
three locales, into `docs/images/site/<locale>/<theme>/<id>.avif` (at most 150 KB each). The
landing shows them at most 1152 CSS px wide, so text stays sharp on Hi-DPI screens. The README
images stay as they are.
