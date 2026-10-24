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
one demo server for each distinct `env`, and takes every shot in each locale (`en`, `zh-TW`,
`zh-CN`) and theme (`light`, `dark`) at twice the pixel density, with reduced motion. It writes:

```text
<out>/<locale>/<theme>/<id>.avif
<out>/manifest.json               every file and its size in bytes
<out>/docs-shots-<version>.tar    both, when a version is given
```

Each AVIF must be at most 150 KB; the script names the files above it and fails. When a shot
fails, Playwright's trace is in `target/docs-shots-results`.

## Declare a shot

```yaml
- id: project-overview          # kebab-case, unique; the file name
  path: /projects/shop          # route in the demo
  wait: 'main h1'               # optional: CSS selector to wait for
  steps:                        # optional, in order
    - click: 'a[href="/projects/shop/services/Deployment/api"]'
    - fill: 'input[name="name"]'
      value: shop-2
    - hover: 'a[href="/projects/shop/settings"]'
    - press: Escape
  clip: main                    # optional: CSS selector to crop to
  viewport: desktop             # desktop (1440×900) or phone (390×844); default desktop
  env:                          # optional: the demo's ZEABUR_DEMO_* variables, as strings
    ZEABUR_DEMO_FORBID: nodes,pods
```

Selectors never use visible text: it changes with the locale. Use a route in `href`, an
attribute, or a `data-testid` added to the component. A selector that matches several elements
uses the first. After the steps the pointer moves to the corner of the page, unless the last step
is `hover`.

The script checks the manifest before it builds anything: unknown keys, ids that are not
kebab-case or not unique, steps other than `click`, `fill`, `hover` and `press`, and `env`
variables outside `ZEABUR_DEMO_*`.

## Same input, same pixels

Two runs on the same machine on the same day give the same files. Two limits:

- The demo dates its objects relative to the moment it starts, so a page that shows a date
  (for example "Created on Sep 26, 2026") changes from one day to the next.
- Fonts render differently on macOS and Linux. The release takes the shots on Linux.
