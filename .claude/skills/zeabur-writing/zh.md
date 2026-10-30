# Chinese (zh-TW and zh-CN)

<!-- This file quotes the patterns it describes. -->
<!-- vale ZeaburEn.Dashes = NO -->

The register is plain written Chinese, as in a good manual: everyday words, full sentences, no 公文腔 and no chat-room clipping. Read every string aloud on its own. If it only makes sense next to the English, rewrite it.

## Rhythm and word choice

- **End a clause on a two-syllable verb.** Written Chinese leans on two-syllable words; a lone one-syllable verb after an adverb (跟著、不會、都) sounds cut off. A one-syllable verb followed by its object is fine (換映像檔、改路徑).

  | Before | After |
  |---|---|
  | 之後換映像檔，排程工作不會跟著換 | 之後更換映像檔，排程工作不會跟著更換 |
  | 裡面的物件都不會變。 | 裡面的物件都不會改變。 |
  | 裡面的東西一個都不會搬。 | 裡面的物件都不會移動。 |
  | 應用程式設定的資料路徑也要跟著改。 | 應用程式設定的資料路徑也要跟著修改。 |

  This is about sound, not length: do not pad with 進行／予以 to get two syllables.
  Basis: a Chinese prosodic foot is two syllables, and a single syllable after a two-syllable word reads tight (劉鴻雁 2017, citing 馮勝利 and 吳為善: https://www.cuhk.edu.hk/ics/clrc/crcl_96_2/liu.pdf).
- **Everyday words, not literal label translations.** "Tight" is 「快不夠了」, not 「偏緊」. "Degraded" is 「部分副本未就緒」, not 「未全數就緒」.
- **Neither 公文腔 nor chat.** Avoid 予以、上述、因此、之、於 in interface text; also avoid 的話、有寫、跑 (for 執行) and 搞定.

  | Before | After |
  |---|---|
  | 這是你唯一的通行密鑰，因此無法移除。 | 這是你唯一的通行密鑰，不能移除。 |
  | 留下的東西要刪掉的話，安裝程式文件有寫怎麼做。 | 要刪除留下的物件，見安裝程式文件。 |
  | 終端機跑在暫時的容器裡 | 終端機在暫時的容器裡執行 |

## Sentence structure (歐化)

- **Drop 它 when the subject has not changed; name the thing when it has.** English needs a pronoun in every clause; Chinese does not.

  | Before | After |
  |---|---|
  | 服務有用到它，但它不是跟服務一起建立的 | 服務有用到，但不是跟服務一起建立的 |
  | 改為「嚴格」：以非 root 使用者執行。它的副本會換成新的。 | 改為「嚴格」：以非 root 使用者執行，副本會換成新的。 |
- **Active voice instead of 被.** Name the doer; with no doer, drop 被. Keep 被 for a real adverse event (權杖被盜用).

  | Before | After |
  |---|---|
  | 一旦用滿，服務會被停掉來釋放記憶體。 | 記憶體用滿時，kubelet 會停掉服務來騰出空間。 |
- **Condition and time first, result last.** English puts *until*, *if*, *to ensure* after the main clause, and that is natural English; Chinese puts them before it. Only the Chinese text gets this rule (`ZeaburZh.ConditionLast`).

  | Before | After |
  |---|---|
  | 上面的服務會停擺，直到節點恢復或副本移走。 | 節點恢復或副本移走之前，上面的服務都會停擺。 |
  | 使用者手上的權杖仍可使用，直到到期，最長為 10 分鐘。 | 使用者手上的權杖到期前仍可使用，最長 10 分鐘。 |
  | 資料會一起刪除，如果叢集設定成保留，管理員還能救回 | 資料會一起刪除；叢集設定成保留時，管理員還能救回 |
  | 這份變更只在幾分鐘內有效，確保套用時仍符合叢集目前的狀態。 | 這份變更幾分鐘後就會過期，套用的內容才會和叢集現況一致。 |
- **The verb itself, not 進行／作出／予以 + noun.** 對設定進行驗證 → 驗證設定；作出決定 → 決定. Keep 進行 when dropping it glues two nouns together: 「上传文件夹进行构建」 stays, because 「上传文件夹构建」 reads as 文件夹构建.
- **One 時 per clause.** 「Zeabur 只在物件還是你檢查時的版本時寫入變更」 → 「物件和你檢查時的版本相同，Zeabur 才會寫入變更」.
- **Break 的 chains and long pre-modifiers** into a clause: 「副本的重新啟動會導致連線的中斷」 → 「副本重新啟動後，連線會中斷」.
- **Calques to rewrite:** 當…的時候 → …時；最…之一 → say it plainly；作為一個 → 是；透過 for a method → 用、在；關於／對於 at the start of a sentence → drop it.

## Say the fact, not the hedge

- **No defensive 不保證／不代表／不宣稱.** Keep a limit only when it changes what the reader does, and put it inside the case it affects, as a positive fact.

  | Before | After |
  |---|---|
  | 這份對照表不代表 Zeabur Identity 已取得 OpenID 認證。 | （刪掉；或寫事實：Zeabur Identity 還沒有申請 OpenID 認證。） |
  | ### 目前不保證的事 ／ 不保證能順利升級或回復 | ### 升級前先知道 ／ alpha 版之間沒有轉換機制，升級或回復都可能失敗 |
  | 這是從你自己的網路檢查，不代表所有人都連得到。 | 這是從你的網路檢查的結果，其他網路可能連不到。 |
- **No 「不是 X，而是 Y」, 「不只是 X，更是 Y」.** Say Y. A contrast stays only when the contrast is the information.

## Punctuation

zh-TW follows the 教育部《重訂標點符號手冊》 (https://language.moe.gov.tw/001/Upload/FILES/SITE_CONTENT/M0001/HAU/Revised_Handbook_of_Punctuation.pdf); zh-CN follows GB/T 15834-2011.

- Full-width punctuation in Chinese text: ，。、；：？！（）. Half-width only inside code, URLs and values.
- Quotes: zh-TW 「」, nested 『』. zh-CN “”, nested ‘’.
- 、 between parallel words (映像檔、環境變數和磁碟區); ， between clauses; ； only where it makes a two-part relation clearer; ： to lead into a list or a value.
- 。 closes one complete thought. Clauses on the same subject or in one cause-and-effect chain join with ，. A paragraph of short 。-sentences reads like stacked English.
- Dash: 「——」 (two em dashes), rarely. Ellipsis: 「……」 in prose; a single 「…」 in interface progress text (正在儲存…).
- No 。 on labels, buttons, titles or table headers.

## Spacing

- One half-width space between Han characters and Latin letters or digits: 「3 個服務」「在 Kubernetes 裡」. Also around inline code in docs: 「執行 `kubectl apply`」.
- No space between Han characters, or next to full-width punctuation: 「沒有 指標儲存服務」 → 「沒有指標儲存服務」.
- No space around bold, italics or links in Chinese text. Write 「按**重新啟動**更換」 and 「見[安裝文件](install.md)設定」, not 「按 **重新啟動** 更換」 or 「見 [安裝文件](install.md) 設定」. Bold Latin text keeps the Han–Latin space: 「用 **Kubernetes** 部署」. `ZeaburZh.SpaceExtra` catches the extra spaces. Where bold text starts or ends with punctuation (「**「設定」**頁」), Markdown needs a space or punctuation outside the `**` to see it as bold; reword so the bold sits between Han characters or punctuation.
- A placeholder that may hold either Chinese or Latin text needs its space computed, not hard-coded (zeabur: `latinStart` / `latinEnd` in `web/src/i18n/zh-TW.ts`).

## zh-CN pass

- Rewrite with mainland terms; do not convert characters. Then check for leftovers: Traditional characters in JSX attributes (`<Tab value="叢集內的 OpenBao">`), Taiwan terms that survive conversion (预设组合 → 默认组合；设定的时间 → 设置的时间).
- Watch words that split into two terms in Taiwan and merge in the mainland. zh-TW has 副本 (replica) and 複本 (copy); in zh-CN both become 副本, so 「${name} 密钥的副本」 now reads as a replica. Write 「复制一份 ${name} 的密钥」.
- Pick one verb per UI action and keep it: 点 or 点击, not both.

## Common terms (generic IT)

Repo glossaries win where they differ (zeabur: `docs/internal/wording.md`, "Terms").

| English | zh-TW | zh-CN |
|---|---|---|
| default | 預設 | 默认 |
| settings | 設定 | 设置 |
| create | 建立 | 创建 |
| file / folder | 檔案／資料夾 | 文件／文件夹 |
| document | 文件 | 文档 |
| data | 資料 | 数据 |
| program | 程式 | 程序 |
| code | 程式碼 | 代码 |
| server | 伺服器 | 服务器 |
| network | 網路 | 网络 |
| memory | 記憶體 | 内存 |
| cache | 快取 | 缓存 |
| support (a feature) | 支援 | 支持 |
| message | 訊息 | 消息 |
| information | 資訊 | 信息 |
| account | 帳號 | 账号 |
| user | 使用者 | 用户 |
| client | 用戶端 | 客户端 |
| sign in / sign out | 登入／登出 | 登录／退出登录 |
| link | 連結 | 链接 |
| object | 物件 | 对象 |
| interface | 介面 | 界面 |
| screen | 螢幕 | 屏幕 |
| menu | 選單 | 菜单 |
| click | 按 | 点击 |
| run (a program) | 執行 | 运行 |
| restart | 重新啟動 | 重启 |
| read-only | 唯讀 | 只读 |
| template | 範本 | 模板 |
| video | 影片 | 视频 |
| quality | 品質 | 质量 |
| software / hardware | 軟體／硬體 | 软件／硬件 |
| print | 列印 | 打印 |
| clipboard | 剪貼簿 | 剪贴板 |
| call (a function) | 呼叫 | 调用 |
| get (a value) | 取得 | 获取 |
| enable | 啟用 | 启用 |
