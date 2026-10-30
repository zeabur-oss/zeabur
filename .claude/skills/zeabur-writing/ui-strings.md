# Interface strings

<!-- This file quotes the patterns it describes. -->
<!-- vale ZeaburEn.Please = NO -->

Applies to labels, buttons, titles, status words, errors, empty states, placeholders and toasts in every language. The repo's own element table (zeabur: `docs/internal/wording.md`, "Element conventions") sets case and punctuation per element.

## Each string stands alone

The reader sees one string at a time, with no English beside it. Read the Chinese string by itself; if it only makes sense as a translation, rewrite it.

| English | Literal (wrong) | Natural |
|---|---|---|
| Tight | 偏緊 | 快不夠了 |
| Degraded | 未全數就緒 | 部分副本未就緒 |
| CPU over-reserved | CPU 快被保留完 | 可保留的 CPU 快用完了 |
| Scaled to 0 | 已縮為 0 | 副本數為 0 |

Short English labels are the hardest to translate: one word in English often needs a whole phrase in Chinese, and the phrase is usually better than a two-character coinage nobody says.

## Patterns by element

| Element | zh-TW | zh-CN | English |
|---|---|---|---|
| Button | The verb and object: 儲存環境變數. No 請, no final punctuation | 保存环境变量 | Save variables |
| Progress | 正在儲存… (never 儲存中…) | 正在保存… | Saving… |
| Failure | 無法載入服務 | 无法加载服务 | Unable to load services |
| Not found | 找不到這個物件 | 找不到这个对象 | Can't find this object |
| Empty state title | What is missing: api 還沒有公開網域 | api 还没有公开域名 | No public domains yet |
| Error body | What happened, then what to do, with the real value or limit | Same | Same |

- **Errors** name the object and the limit ("Choose a name with at most 63 characters"), never just "Invalid name". No blame, no apology, no "Oops".
- **請／please** only in a full sentence that asks the reader to do something inconvenient; never in buttons, labels or titles, and not on every sentence.
- **你, never 您**, and only where the sentence needs a pronoun.

## Building strings

- **One string per sentence, with placeholders.** Do not join fragments; word order differs between languages. A sentence built from parts (lists, counts) needs a helper per language (zeabur: `listZh`, `thingZh`).
- **Plurals and counts** go through the count: `${n} 個服務`, `${n} services`; never "service(s)".
- **Placeholders that can be either script** need computed spacing: 「沒有 指標儲存服務」 came from a hard-coded space before a value that turned out to be Chinese.
- **Leave room.** Chinese labels are often shorter than English, but a natural phrase can be longer than a literal word; check the rendered page at narrow width.
- **Give translators context** in a comment when the key alone does not say where the string appears or what a placeholder holds.
