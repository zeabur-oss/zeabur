---
name: zeabur-writing
description: Write or review text people read in any zeabur-oss repo, in English, zh-TW or zh-CN. Use for docs (.md/.mdx), landing and product page copy, interface strings and i18n catalogues, translations between the three languages, READMEs, CLI output, error messages, and PR or issue text. Includes a Vale style that checks punctuation, spacing, cross-strait terms and wording tells.
---

# Zeabur writing

Text should read like a careful person wrote it in that language: plain words, the reader's task first, natural rhythm. Translation-ese (歐化), filler and defensive hedging are the failures to fix.

Each repo keeps its own term list (zeabur: `docs/internal/wording.md`). Read it before writing; a term listed there beats anything here. [`localization.md`](localization.md) gives the lookup order for the rest and the sources behind these rules.

## Order of work

1. **Know what the reader sees and wants to do.** Open the page, screen or command the text belongs to. Done when you can say, in one sentence, what the reader does next after reading it.
2. **Write in the language you think in.** en, zh-TW and zh-CN are equal: start in any of them, and no locale is the source of the others. Interface strings share one key across the catalogues; the key names the string, it does not fix its wording. Load the reference for each language you write:
   - Chinese, both locales: [`zh.md`](zh.md)
   - English: [`en.md`](en.md)
   - Labels, buttons, errors, empty states, placeholders: also [`ui-strings.md`](ui-strings.md)
   - Landing, product and release pages: also [`landing-copy.md`](landing-copy.md)
3. **Write every other locale as its own text.** Each one reads naturally on its own, as if it had been written first. zh-TW and zh-CN each use their own terms; neither is a character conversion of the other. English is natural US English, and Chinese keeps Chinese word order. All three state the same facts, limits and conditions, and change together in the same pull request. Done when each string makes sense read alone, without the other languages beside it.
4. **Sweep for AI tells** with [`ai-tells.md`](ai-tells.md). Code comments and visuals follow the `no-ai-flavour` skill; a blog post, launch note or technical article also follows `story-writing`.
5. **Check** with Vale (below) and the repo's own checks (zeabur: `scripts/check-zh-tw.sh`), then review with [`review-checklist.md`](review-checklist.md). Done when Vale reports no errors, each warning is fixed or judged wrong for that sentence, and every item of the checklist holds.

When revising someone's text, change the fewest sentences that fix a real problem and report each change as before → after.

## Checking with Vale

The [`vale/`](vale) directory is a Vale style. It catches what a pattern can: punctuation width, spacing between Han and Latin text, quotes, Traditional or Simplified characters in the wrong locale, cross-strait terms, padding verbs, AI-tell words, and paragraphs with several 被 or 它. Rhythm, word order and hedging still need reading.

Run `vale <files>` in the repo's dev shell. A file's language comes from its path: `zh-TW` or `zh-CN` anywhere in it selects the Chinese rules, anything else is English. Add a section to `.vale.ini` for files named otherwise.

**Interface strings** in TypeScript catalogues (`i18n/en.ts`, `i18n/zh-TW.ts`, `i18n/zh-CN.ts`) go through the `ZeaburI18n` view: Vale reads every string and template literal and skips the code, so `${n} 個服務` is checked as two pieces of text around the placeholder. Comments in a catalogue are not checked.

**Silencing a rule** where it is wrong for that text, with the reason next to it:

```md
<!-- Quotes the cluster's own message. -->
<!-- vale ZeaburZh.Passive = NO -->
畫面會顯示「要求被拒絕」，接著顯示「權杖被撤銷」。
<!-- vale ZeaburZh.Passive = YES -->
```

In `.mdx`, write the same as `{/* vale ZeaburZh.Passive = NO */}`. `<!-- vale off -->` and `<!-- vale on -->` silence every rule.

Errors are rules a pattern decides reliably. Warnings point at sentences to read again and do not fail a check: `ZeaburZh.Passive` and `ZeaburZh.Pronoun` (several 被 or 它 in a paragraph), `ZeaburZh.ConditionLast` (a 直到 or 確保 clause after the result; Chinese only, since an *until* clause at the end is natural English), `ZeaburZhTW.Copies` and `ZeaburZhCN.Copies`.
