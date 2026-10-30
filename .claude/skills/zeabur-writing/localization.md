# Localization practice

<!-- The sources quote the words they ban. -->
<!-- vale ZeaburEn.Please = NO -->

Where the terms and rules in this skill come from, and which source wins when they disagree.

## Looking up a term

Take the first source that has it:

1. **The repo's own glossary** (zeabur: `docs/internal/wording.md`).
2. **The platform's own localized term**, for an operating system or browser feature. Use the word the reader sees in the system dialog, not a translation of the English. A passkey is 通行密鑰 on Apple zh-TW and 通行密钥 on zh-CN; iCloud Keychain is iCloud 鑰匙圈 and iCloud 钥匙串 (Apple Support, "About the security of passkeys": https://support.apple.com/zh-tw/102195, https://support.apple.com/zh-cn/102195).
3. **Microsoft Terminology**, for general software terms, searchable or as a .tbx download: https://learn.microsoft.com/en-us/globalization/reference/microsoft-terminology
4. **The generic table** in [`zh.md`](zh.md), "Common terms".

## Sources

- **Microsoft zh-TW and zh-CN localization style guides**: everyday tone, active voice, rewriting instead of translating word for word, spacing between Han and Latin text. https://download.microsoft.com/download/2/9/3/293e6d41-ba40-451b-a41e-94bdb6242ae3/zho-twn-StyleGuide.pdf, https://download.microsoft.com/download/1/5/9/159cb91c-b61b-4385-97ca-80ccc7ff1fa0/zho-chn-StyleGuide.pdf
- **W3C clreq**: the space between Han and Latin text, and 「」 first in Taiwan, “” first in the mainland. https://www.w3.org/TR/clreq/
- **Apple Style Guide and HIG Writing**: no *please* in instructions, verbs on buttons, no *we*, errors that neither blame nor say "oops". https://support.apple.com/guide/applestyleguide/p-apsg4473eab0/web, https://developer.apple.com/design/human-interface-guidelines/writing
- **Google, "Write for a global audience"**: short plain sentences and consistent terms, so text in any locale carries over to the others. https://developers.google.com/style/translation
- **Mozilla, localization best practices**: no string concatenation, context for translators, plurals through a selector, room for longer text. https://mozilla-l10n.github.io/documentation/localization/dev_best_practices.html
- **Android, "Localize your app"**: a comment beside each string with its purpose, place and length limit; placeholders marked as not for translation. https://developer.android.com/guide/topics/resources/localization

## Where we differ on purpose

- We write 你 in both locales. Microsoft's zh-TW guide uses 您.
- At most one em dash aside per English sentence is our own rule. Google and Microsoft allow unspaced em dashes.
