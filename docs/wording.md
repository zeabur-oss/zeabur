# Wording

How the interface's text reads in English, Traditional Chinese (Taiwan) and Simplified Chinese.
It covers `web/src/i18n/` (English is the source; `zh-TW.ts` and `zh-CN.ts` are typed against
it), the English messages the API sends with a problem, the sign-in pages `zeabur` serves, and
`zeaburctl`'s output. *What* a sentence says comes from what people see on the page, not how
Zeabur works inside; this guide is about *how* it reads.

The reader is a developer or a small team deploying apps on their own cluster. Apple's writing
guidance is the primary voice; Google and Microsoft fill gaps; for Chinese, Chinese-specific
sources come first [A1][A6][G2][M1][M4][M5]. Linters and style scores point at text worth
rereading; they do not decide it.

## All three locales

1. **Lead with the fact or the action.** No sentence that announces the page or restates its
   label ("This page shows…", "Here you can…").
2. **Name the doer.** "The cluster rejects this change", not "This change will be rejected".
   Passive only when the actor is unknown or does not matter.
3. **No "we", few possessives.** "Unable to load services", not "We couldn't load your
   services". Keep "your" where it tells the reader's thing from another ("your own rights").
4. **Errors say what failed, on which object, and what to do next**, with the actual value and
   limit: "Choose a name with at most 63 characters", not "Invalid name". Do not guess a cause,
   echo a secret, blame, apologise or add "Oops".
5. **Alert titles describe the situation; buttons name the result.** Never "Error" as a title;
   "Delete PVC" / "Cancel", not "OK" / "Yes". Body text only when it adds a consequence; do not
   explain the buttons.
6. **Limits where they bite.** Warn only before an uncommon or irreversible action, and state the
   real consequence. Whether deleting a PVC deletes data depends on its reclaim policy; say what
   is known.
7. **Empty states give the next step**, with a button or link where one exists. Nothing the
   reader needs may live only in text that disappears once the list fills.
8. **A setting's label says what "on" does.** Link to a setting rather than describe where it is.
9. **Ordinary words.** Reread *just, simply, easy, please, in order to, desired, via, leverage,
   allows you to*, and needless *there is* / *you can*; keep one only when it carries meaning.
10. **No manufactured structure.** A contrast ("not X but Y", 「不是…而是…」) only when the
    contrast is the information; three items only when there are three; no closing summary.
11. **One term per concept**, from the term list below. Kubernetes kinds and fields (Deployment,
    ConfigMap, selector, `replicas`) stay as they are and may repeat. Interpolations, numbers,
    defaults and code stay exact.

### English

Natural US English (*catalog*, not *catalogue*), contractions where they fit, serial comma.
Reread inflated words (*delve, underscore, showcase, highlight, crucial, comprehensive, robust,
seamless, pivotal, notably, additionally*), participle tails (", ensuring…"), *serves as* for
*is*, and nominalisations ("performs validation of" → "validates") [R3][R5]. These are review
flags, not bans.

## Chinese

Write each locale for its readers; translate intent and conditions, not English word order.
zh-TW and zh-CN are two editorial passes, not a character conversion.

12. **Active voice instead of 被**, with the agent when known: 「叢集會拒絕這個變更」. With no
    agent, drop 被: 「檔案會自動儲存」. Keep 被 for a real adverse event (a token was stolen) and
    where dropping it would change who acts.
13. **Plain verbs.** 「驗證設定」, not 「對設定進行驗證」; 「是」「有」, not 「作為」「擁有」.
    Drop 「有關」「關於」「們」「一個」 when they add nothing.
14. **An event or clause as subject; break 的 chains and long pre-modifiers.** 「Pod 重新啟動後，
    連線會中斷」, not 「Pod 的重新啟動會導致連線的中斷」.
15. **Chinese order.** Condition first (「如果…，…」), one idea per sentence; no copied pairs
    (因為…所以, 當…的時候), no 「最…之一」「值得注意的是」「這意味著」. 「；」 is allowed where it
    makes the relation clearer; split the sentence when it does not.
16. **Fixed patterns.** Failure 「無法…」／「无法…」; missing 「找不到…」; recommendation
    「建議…」／「建议…」; progress 「正在儲存…」／「正在保存…」, not 「儲存中…」.
17. **Pronouns.** Omit them where the meaning survives; otherwise 你 in both locales, never 您.
    請／请 only in a full sentence that asks the reader to act, and not on every one of those;
    never in a button, label or title.
18. **Vocabulary and punctuation.** zh-TW: Taiwan terms, full-width punctuation, 「」 (nested
    『』), a half-width space between CJK and Latin letters or digits. zh-CN: mainland terms,
    “” (nested ‘’), the same spacing, 「如果」 for 「倘若」, 「要」 for 「若要」.
    `scripts/check-zh-tw.sh` checks zh-TW vocabulary and punctuation; take a suggestion only when
    it fits the technical meaning.

## Element conventions

Source text, not a CSS transform. Product, API and code names keep their exact case.

| Element | English | zh-TW / zh-CN |
|---|---|---|
| Navigation, tabs, labels, table headers, badges, status words | Sentence case, no final period or colon: `Environment variables`, `Rolling out` | Short label, no final 。 or ： |
| Buttons and menu items | Sentence case, the action or destination, no period: `Delete service`, `Open logs` | The action, no final punctuation, no 請 |
| Dialog and alert titles, section headings | Sentence case, the situation or the object, no period: `Restart api`, `Needs attention` | Same; ？ only for a real question |
| Body, helper text, error details, tooltips | Full sentences end with a period | Full sentences end with 。 |
| Empty states and one-line notes | A sentence, ending with a period: `No volumes.` | Ends with 。 |
| Inline fragments (a word that continues a value or another string: `250Mi requested`, `, required`) | Lower case, no period | No punctuation |
| Progress | `Saving…` (one … character) | 「正在儲存…」／「正在保存…」 |
| Placeholders | An example value, not an instruction: `app.example.com` | Same |
| Counts | The number and the noun: `3 services`, `2/3 ready` | 「3 個服務」／「3 个服务」 |
| Code, values, kinds | Exact | Exact; never full-width |

## Names

- **Zeabur** is the product, always capitalised. Its parts are **Zeabur Identity** (sign-in) and
  **Zeabur Catalog** (tested component versions and how to install them). In Chinese the names
  stay in Latin letters.
- A *service* is what the reader deploys (服務／服务); `Service` is the Kubernetes kind. A
  *project* is a namespace Zeabur shows as one (專案／项目). A *replica* is a pod of a service
  (副本); `Pod` stays where the page is about the cluster rather than one service.
- Kubernetes kinds stay in Latin letters in Chinese, Namespace included, as Zeabur's hosted
  dashboard writes it; in English running text they are lower case unless the kind itself is
  meant (`the Namespace shop`).

## Terms

One choice per concept. Where Zeabur's hosted dashboard names the same thing on screen, this
interface uses its word unless it breaks a rule above; the last column says where it differs and
why. Add a row before introducing a new concept.

| English | zh-TW | zh-CN | Note |
|---|---|---|---|
| project | 專案 | 项目 | |
| service | 服務 | 服务 | |
| cluster | 叢集 | 集群 | |
| node | 節點 | 节点 | |
| namespace | Namespace | Namespace | Kubernetes kind, as the dashboard writes it |
| replica | 副本 | 副本 | |
| container | 容器 | 容器 | |
| image | 映像檔 | 镜像 | |
| volume | 磁碟區 | 卷 | The dashboard also says 硬碟／硬盘; a claim is not a disk |
| port | 連接埠 | 端口 | |
| domain | 網域 | 域名 | |
| environment variable; Variables tab | 環境變數 | 环境变量; tab 变量 | Tab names as on the dashboard |
| Usage tab, resource usage | 用量 | 用量 | Dashboard's Usage tab |
| Overview | 概覽 | 概览 | |
| Settings | 設定 | 设置 | |
| General (settings section) | 一般設定 | 通用设置 | |
| Description | 描述 | 描述 | |
| Danger zone | 危險區域 | 危险区域 | |
| secret (a Secret, in plain words) | 機密 | 机密 | 密钥 is a key |
| config file (a ConfigMap, in plain words) | 設定檔 | 配置文件 | |
| configuration | 設定 | 配置 | |
| logs | 日誌 | 日志 | |
| event | 事件 | 事件 | |
| warning | 警告 | 警告 | |
| metrics | 指標 | 指标 | |
| reserved (a request) | 保留量 | 预留量 | |
| maximum (a limit) | 上限 | 上限 | |
| memory | 記憶體 | 内存 | |
| component | 元件 | 组件 | |
| catalog | 目錄 | 目录 | |
| policy | 政策 | 策略 | |
| permission, rights | 權限 | 权限 | |
| admin rights | 管理員權限 | 管理员权限 | |
| session | 工作階段 | 会话 | |
| credential | 認證資訊 | 凭证 | zh-TW 憑證 is a certificate |
| certificate | 憑證 | 证书 | |
| token | 權杖 | 令牌 | |
| passkey | 通行密鑰 | 通行密钥 | Apple's name in the system dialog that asks for it (Google's zh-TW wording differs; not verified); chosen over the dashboard's Latin "Passkey" so the page and that dialog match |
| sign in / sign out | 登入／登出 | 登录／退出登录 | |
| deploy, roll out | 部署 | 部署 | |
| releases (a service's revisions) | 發布紀錄 | 发布记录 | |
| revision | 修訂版本 | 修订版本 | |
| restart | 重新啟動 | 重启 | |
| roll back | 回滾 | 回滚 | |
| apply a change | 套用 | 应用 | |
| review (a plan) | 預覽 | 预览 | |
| run a command | 執行 | 运行 | |
| adopt (show a namespace as a project) | 納入 | 纳入 | |
| remove from Zeabur | 移除 | 移除 | |
| read-only | 唯讀 | 只读 | |
| view (compact action) | 檢視 | 查看 | |
| jobs (Jobs and CronJobs together) | 批次工作 | 批处理任务 | One-off Jobs are not scheduled |
| scheduled job | 排程工作 | 定时任务 | | A CronJob |
| background job | 背景工作 | 后台任务 | |
| health check | 健康檢查 | 健康检查 | |
| traffic check (readiness) | 就緒檢查 | 就绪检查 | |
| restart check (liveness) | 存活檢查 | 存活检查 | |
| light / dark theme | 淺色／深色 | 亮色／暗色 | |

## Examples

| Before | After | Why |
|---|---|---|
| This page allows you to perform inspection of your services. | Inspect services. | Action first; no restated label or nominalisation |
| This change will be rejected by the cluster. | The cluster rejects this change. | Name the doer |
| We couldn't load your services. | Unable to load services. | No "we"; no needless possessive |
| Invalid name. | Choose a name with at most 63 characters. | The real limit |
| 這個變更會被叢集拒絕。 | 叢集會拒絕這個變更。 | 主動語態 |
| 對設定進行驗證 | 驗證設定 | 直接用動詞 |
| 您可以在此處查看您的服務。 | 檢視服務 | 不用代名詞與頁面說明 |
| 儲存中… | 正在儲存… | 固定的進度句型 |
| 倘若您希望查看您的服务，请前往服务页面。 | 要查看服务，打开服务页面。 | 去掉公文腔；能放連結就放連結 |

## Review

Read changed text on the rendered page, in every locale, at desktop and narrow widths: wrapping,
truncation, mixed CJK and Latin, buttons that overflow. Check that the three locales state the
same facts, limits and conditions. `bun run typecheck`, `bun run test` and
`scripts/check-zh-tw.sh` must pass; a passing check is not the review.

## Sources

- [A1] Apple, "Writing," *Human Interface Guidelines*. https://developer.apple.com/design/human-interface-guidelines/writing
- [A6] K. Coon and J. Bush, "Writing for interfaces," WWDC22, session 10037. https://developer.apple.com/videos/play/wwdc2022/10037/
- [G2] Google, "Highlights," *Google developer documentation style guide*. https://developers.google.com/style/highlights
- [M1] Microsoft, "Top 10 tips for Microsoft style and voice." https://learn.microsoft.com/en-us/style-guide/top-10-tips-style-voice
- [M4] Microsoft, *Chinese (Traditional) Localization Style Guide*, 2024. https://download.microsoft.com/download/2/9/3/293e6d41-ba40-451b-a41e-94bdb6242ae3/zho-twn-StyleGuide.pdf
- [M5] Microsoft, *Chinese (Simplified) Localization Style Guide*, 2024. https://download.microsoft.com/download/1/5/9/159cb91c-b61b-4385-97ca-80ccc7ff1fa0/zho-chn-StyleGuide.pdf
- [R3] A. Reinhart et al., "Do LLMs write like humans?," *PNAS* 122(8), 2025. https://pmc.ncbi.nlm.nih.gov/articles/PMC11874169/
- [R5] WikiProject AI Cleanup, "Wikipedia:Signs of AI writing." https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing
