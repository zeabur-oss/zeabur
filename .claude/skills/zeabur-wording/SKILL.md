---
name: zeabur-wording
description: Zeabur's own terms and steps for any text people read in Zeabur — interface strings in web/src/i18n (en, zh-TW, zh-CN), API problem messages, sign-in pages, zeaburctl output, README and docs pages. Use whenever a change adds, renames or rewrites a user-visible string or a docs page, or when reviewing copy.
---

# Wording

Zeabur's terms and conventions for English, zh-TW and zh-CN. `docs/internal/wording.md` is the
rulebook.

1. **Name the thing by its plain term first**; the Kubernetes term follows as secondary text only
   where a professional needs it (`docs/internal/wording.md`, "Terms"). Never invent a term; use the
   glossary's (通行密鑰、機密變數、磁碟、磁碟區、用量、概覽…).
2. **`web/src/i18n/en.ts` defines the keys**; zh-TW and zh-CN are typed against it. Start in
   whichever locale you think in, write each one so it reads on its own, and change all three in
   the same pull request. Use `common.colon` instead of a literal colon.
3. **Empty states and absences say what is missing and what to do**, never a bare 「沒有。」/
   "None." A section that has nothing is usually hidden or says why and offers the action.
   Section titles name their content (公開網域、私有網路), not their position (「在這裡設定」).
   Show the real value with a copy button (`api.shop.svc.cluster.local:8080`), not a description
   of it. Kubernetes terms go in the design system's `Term` tag, never a made-up middle word
   (內部位址).
4. **Errors** keep the raw cause available (details, copy) but out of the headline.
5. **Check**, inside `nix develop`:
   ```sh
   (cd web && bun run typecheck && bun run test)
   nix develop .#zh -c ./scripts/check-zh-tw.sh
   ```
   Then read the strings on the rendered page in all three
   locales at desktop and narrow widths (`zeabur-ui` for the demo server).

Moving a hard-coded string into the catalogues is part of the change, not a follow-up.

## Docs pages

`docs/` (en), `docs/zh-TW/` and `docs/zh-CN/` are written for someone who has used zeabur.com
but not Kubernetes. A feature's pull request brings its page in this shape.

1. **zh-TW first.** Write the zh-TW page, then en and zh-CN as their own texts with the same facts,
   steps and screenshots in the same places. All three change in the same pull request.
2. **A task page has three parts, in this order:** what the reader has at the end (a sentence or
   two, and the finished screen when it helps), the numbered steps, then what to do when something
   goes wrong (what the reader sees, and what to do about it). Reference pages keep their shape.
3. **A screenshot right after each step that changes the screen**, in all three locales. Take it
   with a shot in `scripts/shots/docs.ts` and `scripts/docs-shots.sh` (`docs/internal/screenshots.md`),
   never by hand, so the next run updates it. Its alt text says what the reader should see. A step
   that only copies a value or happens outside Zeabur needs none. A screen another open issue will
   change gets a sentence on what it is for, not a step-by-step shot, until that issue lands.
4. **One idea per sentence, in everyday words.** Say what the reader does and sees: 「按**部署**」,
   not 「觸發部署流程」. Name buttons and fields as the screen does, in bold.
5. **No Kubernetes term before the reader needs it.** Use the plain word from
   `docs/internal/wording.md` (副本, 磁碟, 機密變數). Bring in the Kubernetes term only where the
   reader acts on it (`kubectl`, YAML, an install step), with one short sentence the first time
   it appears; after that, use it bare. A "with kubectl" section goes at the end of the page.
6. **A diagram before long text.** A concept, or a page whose idea takes more than a short
   paragraph, opens with one diagram, then short text. Draw it as an SVG file for each locale,
   `docs/[<locale>/]images/diagrams/<id>.svg`, shown like a screenshot, so it renders on GitHub and
   on the site. Use only the colours, strokes, corners and fonts in
   [`docs/internal/diagrams.md`](../../../docs/internal/diagrams.md): violet only for the one thing
   that matters, no shadows, glow, gradients or window dots (`no-ai-flavour`). Keep it to about
   seven boxes; the alt text states the idea.
7. **Install pages follow who installs.** Most people paste a prompt into Claude or Codex and let
   it install. Hands-on users run the Zeatend TUI or CLI. Cluster admins who want to see every
   object install by hand (Helm, kubectl, Flux or Argo CD). Pages and the sidebar keep that order,
   and each install page links to the other two in one line. A path that has not shipped gets a
   callout that it is in development, a description of what is coming and its issue, and no
   commands or prompts to follow yet.
