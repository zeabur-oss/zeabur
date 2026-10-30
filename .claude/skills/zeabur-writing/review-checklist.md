# Review checklist

Work through every item for every changed string or paragraph. The review is done when each item holds; a clean Vale run alone is not the review.

1. **Vale.** `vale <changed files>` reports no errors. Each warning is fixed, or you read the sentence and the rule does not fit it. A silenced rule has a comment saying why.
2. **Repo checks.** The repo's own wording checks pass (zeabur: `nix develop .#zh -c ./scripts/check-zh-tw.sh`). Take a suggestion only when it fits the technical meaning.
3. **Alone.** Each Chinese string makes sense without the English next to it. Each label is a word or phrase a person would say.
4. **Aloud.** Read each Chinese sentence aloud. No clause ends on a clipped one-syllable verb; no 它 where the subject has not changed; no condition or *until* clause after the result.
5. **Fact, not hedge.** No sentence says only what is not guaranteed or not implied. Every warning changes what the reader does.
6. **Terms.** Every term matches the repo's term list, in each locale, and system features use the platform's own term ([`localization.md`](localization.md)). zh-CN has no Traditional characters or Taiwan terms; zh-TW has no mainland terms.
7. **Same facts.** The three languages carry the same values, limits, conditions and links. None adds or drops a caveat.
8. **On the page.** Read the strings where they render, in each locale, at desktop and narrow width: wrapping, truncation, mixed CJK and Latin spacing.
9. **Report.** List every changed sentence as before → after.
