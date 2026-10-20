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
- **Design.** agent-playground's and lymeno's language (Linear's: near-hueless greys, 0.5px
  hairlines, faint shadows, Inter 13px at 450, controls rounded 8 and popups 12, no focus rings)
  with Zeabur's lime and ink as the accent, in agent-playground's frame (the design
  system's `AppFrame`, `Sidebar`, `PanelHeader`). Build from the design system's components and
  their variants rather than look-alikes. Tokens, button and frame rules and banned styles are in
  `web/DESIGN.md`; `scripts/check-ui-styles.sh` enforces the bans (including focus rings).
- **Lint.** `bun run lint` includes [shadcn/lint](https://github.com/shadcn-ui/lint) (rules in
  `web/.oxlintrc.json`, which recognises the design system's components by `componentImports`;
  conventions in `web/DESIGN.md`, "Lint"). Pages pass only layout classes to a component: a
  repeated treatment becomes a cva variant or size in the design system, not a className at each
  call site. No arbitrary values (`type-*` roles, the spacing scale, or a named token); `style`
  carries only CSS custom properties, read by classes such as `left-(--x)`.
- **Budgets.** `scripts/ui-budget.sh` checks the brotli size of what the first page load needs;
  raise a budget only with a reason in the commit. Menus, popovers, tooltips and dialogs load
  after the first page (the design system's `popup.tsx`): use them through the design system, never
  `@base-ui/react/{menu,popover,tooltip,dialog}` directly, which would put them back on it.
