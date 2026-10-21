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
  (cd web && bun ci && bun run fmt:check && bun run lint && bun run knip && bun run test)
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
  cluster, the person and admin rights), with no top bar; inside a project its own sidebar
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
  it (`model/overview.ts`). Projects are cards (`components/project-card.tsx`); settings are `SectionLabel` over
  `Card variant="list"` of `CardRow`s. No underline tabs. Tokens, layout, shell and banned styles are in
  `web/DESIGN.md`; `scripts/check-ui-styles.sh` enforces the bans (including focus rings).
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
