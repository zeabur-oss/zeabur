---
name: zeabur-wording
description: Zeabur's own terms and steps for any text people read in Zeabur — interface strings in web/src/i18n (en, zh-TW, zh-CN), API problem messages, sign-in pages, zeaburctl output, README and docs. Use whenever a change adds, renames or rewrites a user-visible string, or when reviewing copy.
---

# Wording

Follow `zeabur-writing` (`skills-lock.json` pins it) for how text reads in English, zh-TW and
zh-CN. This skill adds Zeabur's terms and conventions; `docs/internal/wording.md` is the
rulebook, and its term beats anything in `zeabur-writing`.

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
   vale <changed files>
   (cd web && bun run typecheck && bun run test)
   nix develop .#zh -c ./scripts/check-zh-tw.sh
   ```
   plus the `zeabur-writing` checks. Then read the strings on the rendered page in all three
   locales at desktop and narrow widths (`zeabur-ui` for the demo server).

Moving a hard-coded string into the catalogues is part of the change, not a follow-up.
