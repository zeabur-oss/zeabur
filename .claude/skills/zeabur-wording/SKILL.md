---
name: zeabur-wording
description: Write or revise any text people read in Zeabur — interface strings in web/src/i18n (en, zh-TW, zh-CN), API problem messages, sign-in pages, zeaburctl output, README and docs. Use whenever a change adds, renames or rewrites a user-visible string, or when reviewing copy.
---

# Wording

`docs/internal/wording.md` is the rulebook; read it before writing. This skill is the order of work.

1. **Start from what the reader sees and wants to do**, not from how Zeabur or Kubernetes works.
   Name the thing by its plain term first; the Kubernetes term follows as secondary text only
   where a professional needs it (`docs/internal/wording.md`, "Terms"). Never invent a term; use the
   glossary's (通行密鑰, 機密, 磁碟區, 用量, 概覽 …).
2. **Write English first** (`web/src/i18n/en.ts` is the source; zh-TW and zh-CN are typed against
   it). Sentence case, the verb on the button, a label that names its content.
3. **Write each Chinese locale as its own text**, not a translation of the English: Taiwan and
   Mainland vocabulary differ (重新啟動 / 重启, 磁碟區 / 卷). Avoid 被 passives, a 請 on every line,
   「進行」 padding and full stops on labels. Use `common.colon` instead of a literal colon.
4. **Empty states and absences say what is missing and what to do**, never a bare 「沒有。」/
   "None." A section that has nothing is usually hidden or says why and offers the action.
   Section titles name their content (公開網域, 私有網路), not their position (「在這裡設定」).
   Show the real value with a copy button (`api.shop.svc.cluster.local:8080`), not a description
   of it. Kubernetes terms go in the design system's `Term` tag, never a made-up middle word
   (內部位址).
5. **States in progress** read 「正在…」/ "Restarting…"; results say what changed.
6. **Errors**: what happened, in the user's terms, then what they can do. Keep the raw cause
   available (details, copy) but out of the headline.
7. **Check**, inside `nix develop`:
   ```sh
   (cd web && bun run typecheck && bun run test)
   nix develop .#zh -c ./scripts/check-zh-tw.sh
   ```
   Then read the strings on the rendered page in all three locales at desktop and narrow widths
   (`zeabur-ui` for the demo server). A passing check is not the review.

Moving a hard-coded string into the catalogues is part of the change, not a follow-up.
