---
name: zeabur-dependencies
description: Add or upgrade a Rust or web dependency in Zeabur under its supply-chain policy (OSI licences, aws-lc-rs only, seven-day publish age, unsafe/native review, bun.lock). Use whenever Cargo.toml, Cargo.lock, web/package.json, web/bun.lock or flake.nix changes.
---

# Dependencies

Follow `zeabur-oss-dependencies` (install: `npx skills add zeabur-oss/skills --skill
zeabur-oss-dependencies`; `skills-lock.json` pins it). This skill adds what is Zeabur's own.

## Rust

- The unsafe report is `./scripts/ci.sh unsafe-report`. Only normal and build dependencies of
  `zeabur`, `zeaburctl`, `zeabur-proxy` and `zeabur-installer` count. kube's and axum's `ws`,
  for the terminal, brought in `tungstenite` when they moved from dev-dependencies to normal
  ones: that move needed the review.
- `k8s-openapi` is built for Kubernetes 1.34 (`v1_34`), the oldest version Zeabur supports.

## Web (`web/`)

- Add packages from `web/`.
- The one licence exception is `caniuse-lite` (CC-BY-4.0 data, pulled in by svgr's Babel,
  development only; see CONTRIBUTING.md).
- Icons are Phosphor's, from `@iconify-json/ph` (built in by unplugin-icons with its JSX compiler,
  `@svgr/core` + `@svgr/plugin-jsx`; all dev dependencies). Components come from the shadcn
  registry (`bunx --bun shadcn@latest add <component>`), on Base UI.
- `@zeabur/design-system` is checked in as `vendor/zeabur-design-system-<commit>.tgz`, so that
  `bun ci` downloads nothing from GitHub; `scripts/bump-design-system.sh <commit>` moves it.
- Check the first-load size after adding a runtime package: `./scripts/build-ui.sh &&
  ./scripts/ui-budget.sh`.
