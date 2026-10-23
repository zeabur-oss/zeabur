---
name: zeabur-ui
description: Change Zeabur's web interface (web/: React, TanStack, shadcn/ui on Base UI, Phosphor) and its embedding (crates/zeabur-ui) while keeping the strict CSP, size budgets and the design system's tokens (@zeabur/design-system). Use for any change under web/ or crates/zeabur-ui.
---

# Web interface

- **Where things are.** Sources: `web/src` (routes in `router.tsx`). Components (shadcn/ui on
  Base UI with Zeabur's variants), theme, tokens and fonts come from `@zeabur/design-system`
  (github.com/zeabur/design-system, a Git dependency pinned to a commit in `web/package.json`;
  import `@zeabur/design-system/components/<name>`; preview in its Storybook). Change a
  component or token there, then move the pin (`bun add 'github:zeabur/design-system#<commit>'`).
  `web/src/styles/app.css` imports it and adds only this interface's layout tokens. Dark mode
  follows the operating system (`src/theme.ts` sets shadcn's `.dark` class; pass its `useTheme()`
  to the design system's `Toaster`). Embedding and serving: `crates/zeabur-ui`.
- **Build and check** (inside `nix develop`):
  ```sh
  (cd web && bun ci && bun run fmt:check && bun run lint && bun run test)
  (cd web && bun run fallow dead-code --baseline fallow-baseline.json && bun run fallow audit --base main)
  ./scripts/check-ui-styles.sh && ./scripts/build-ui.sh && ./scripts/ui-budget.sh
  ZEABUR_UI_DIST="$PWD/web/dist" cargo nextest run -p zeabur-ui
  nix develop .#zh -c ./scripts/check-zh-tw.sh   # zh-TW wording, after changing a zh-TW string
  ```
  The build output (`web/dist`) is never committed. Develop with `bun run dev` in `web/` and
  `cargo run -p zeabur-api --example demo` (an in-memory cluster; Vite forwards `/api`).
- **CSP rules.** No inline `<script>`, `<style>`, `style=""` in HTML or `on*=` handlers
  (`style-src` allows `'unsafe-inline'` only for the `<style>` elements Sonner and CodeMirror
  insert; `web/DESIGN.md`, "Content Security Policy"); no URLs
  to other origins (fonts are self-hosted); no `dangerouslySetInnerHTML` (Trusted Types
  `'none'`). `web/scripts/postbuild.ts` rejects an inline script or style in `index.html`.
- **Icons.** Phosphor only, one import per icon: `import XIcon from "~icons/ph/x"` (a weight is
  its own name, `~icons/ph/warning-fill`), built at build time from `@iconify-json/ph` by
  unplugin-icons (its JSX compiler, through svgr); decorative ones with `aria-hidden`.
- **Design.** The frame is the design system's (`AppFrame`, `FrameSidebar`, `Sidebar` with
  `SidebarSearch`, `SidebarMenu`, `SidebarCopyPill`, and `ContextChip` at the foot for the
  cluster, the person and admin rights; Home and Projects, then Cluster, Members and Settings
  under Manage; the chip's menu links Account), with no top bar; inside a project its own sidebar
  (`components/shell/project-sidebar.tsx`) replaces the global one. Home is "What needs you"
  (incidents, then problems by origin, each with cause, fixer and fix). A project is one page:
  Needs attention, then its map or list (`?view`) with Activity, gaps behind "N suggestions". A
  service page is a header and seven segmented `Tabs` named as hosted Zeabur names them (Overview,
  Logs, Deployments, Usage, Networking, Variables, Settings) in `max-w-content`; Settings is one
  page with a `SectionNav`, and edits in Networking, Variables and Settings collect in one
  `DraftBar`. The Kubernetes view of a tab is a second segment inside it (`?mode=`: Logs'
  Terminal, Deployments' Events, Settings' YAML). The header's ⋯ menu holds actions only (Copy
  diagnosis, Delete), never a view; a tab is never also a header button. An old `?tab=` must keep
  redirecting (`routes/search.ts`). Overview leads with one sentence each: health, then each domain
  reachable, not reachable with its fix, waiting on the cause above, or not verified (never red
  for a hop Zeabur could not check); each cause is said once, with what it also takes down inside
  it (`model/overview.ts`). Projects are a compact table by default, cards on request
  (`components/project-card.tsx`); settings are `SectionLabel` over
  `Card variant="list"` of `CardRow`s. No underline tabs. Tokens, layout, shell and banned styles are in
  `web/DESIGN.md`; `scripts/check-ui-styles.sh` enforces the bans (including focus rings).
- **Who it is for.** Most people self-hosting run one small server with a few services and do not
  know Kubernetes: they need to know whether things are fine, why not, and what to do. A minority
  runs dozens of services and wants raw detail. Serve both with one screen: conclusions by default,
  density and Kubernetes detail one step behind. Keep hosted Zeabur's nouns, tabs and places
  (familiarity lives there); change how much is shown and how it is grouped.
- **Principles.** Check every screen against these before calling it done.
  1. *Conclusion first* (Nielsen 1, 9): every page and every collection says whether it is fine,
     the cause when not, the next step, and what that step costs. A red dot without a cause is
     unfinished.
  2. *True status* (Norman's gulf of evaluation): show only what Zeabur observed. Unknown or
     unreachable is not failing. When the cluster cannot be read, show what is still known (the
     host, the last events) rather than a page of dashes. An operation's result comes from the
     cluster, not from the request that started it.
  3. *Design for 0, 1, few and many*: define each collection at 0, 1, up to about 7, and 50+.
     Sentences and chips are for few. Many becomes a summary grouped by outcome (exceptions first,
     with counts) that expands into a sortable, filterable table. Never print the same verdict
     once per item (Tufte: no redundant ink). This includes Plan and Apply: a handful of changes
     may read as sentences; more are grouped by object and kind (added, changed, removed, with
     counts) and listed as field, before, after.
  4. *Overview, filter, details* (Shneiderman): a long collection scrolls inside a bounded frame
     with a sticky header instead of pushing the rest of the page away; working sets are not
     paginated. Counts next to section names say what is below the fold.
  5. *Defaults follow the data*: the view, its density and what is expanded depend on the data
     and the screen, not on a toggle people must find. The project page opens on the map only
     when the map, fitted to the screen, can be read (zoom 0.75 or more, at most 16 services; on
     a phone never); else on the list. A remembered choice never overrides that rule.
  6. *Visible entry points* (Norman): a frequent action is a labelled button where people look
     for it, not only inside ⋯ or another tab.
  7. *Accelerators that novices never see* (Nielsen 7): `/` to filter, ⌘K, sortable headers, state
     in the URL. They never change the default screen.
  8. *Say the cost first* (Nielsen 5): before restart, upgrade, delete or reinstall, say whether it
     stops traffic, whether data stays and whether it can be undone.
  9. *Phones triage*: status and problems first, one key fact per row; dense tables and matrices
     become a summary and a search box, not a squeezed desktop.
  10. *Check at scale*: screenshot every changed collection at 390, 1440 and 1960 with the demo's
      small data and with many items (dozens of services, rules, domains, changes, long names).
- **fallow** (`bun run fallow <command>`, the version pinned in `web/package.json`, config in
  `web/.fallowrc.json`).
  - `dead-code`: unused files, exports and dependencies, and imports that cross a layer. Layers go
    upwards only: `i18n` ← `session` ← `api` ← `lib` ← `auth` ← `model` ← `components` ←
    `modules` ← `routes` ← `main.tsx`/`router.tsx`. Unit tests may import any layer. A new
    directory under `src/` needs a zone. `fallow-baseline.json` holds the findings that predate
    fallow; never add to it. After a fix, `bun run fallow baselines prune` drops its entry.
  - `audit --base main`: what CI gates on. It fails on findings that the change introduces in the
    files it touches: dead code, complexity over the thresholds (cyclomatic 20, cognitive 15,
    CRAP 30) and new duplicated blocks. Inherited findings are listed but do not fail.
  - `health`: the score (0–100) and its `penalties`, complexity per function, and hotspots
    (complex files that change often). `dupes`: clone groups. `health --css`: CSS structure and
    token drift; it reads stylesheets, not Tailwind classes. These are reports, not gates.
  - Read a finding from its `--format json` output: `actions` says the fix. A false positive
    gets an entry in `.fallowrc.json` with the reason, as for the lazy route pages. Do not add
    `fallow-ignore` comments to hide a true finding.
- **Lint.** `bun run lint` includes [shadcn/lint](https://github.com/shadcn-ui/lint) (rules in
  `web/.oxlintrc.json`, which recognises the design system's components by `componentImports`;
  conventions in `web/DESIGN.md`, "Lint"). Pages pass only layout classes to a component: a
  repeated treatment becomes a cva variant or size in the design system, not a className at each
  call site. No arbitrary values (`type-*` roles, the spacing scale, or a named token); `style`
  carries only CSS custom properties, read by classes such as `left-(--x)`. No `@base-ui/react`,
  Radix, Headless UI or Motion imports (`no-restricted-imports`): add the component to the design
  system instead.
- **Budgets.** `scripts/ui-budget.sh` checks the brotli size of what the first page load needs;
  raise a budget only with a reason in the commit. Menus, popovers, tooltips and dialogs load
  after the first page (the design system's `popup.tsx`): use them through the design system, never
  `@base-ui/react/{menu,popover,tooltip,dialog}` directly, which would put them back on it.
