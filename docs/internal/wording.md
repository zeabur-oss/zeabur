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
   explain the buttons. The button that applies a review follows one rule, whether a form, YAML,
   a drawer or the draft asked: one kind of change takes its verb ("Restart 3 services", "Save
   variables"); changes that only create say "Create", with the count when there are more than
   one ("Create 3 objects"); several kinds together say "Apply 4 changes". A form whose objects
   are only the means (database backups) says the change in its own words and takes its own verb
   ("Set up backups"), not the object edits.
6. **Limits where they bite.** Warn only before an uncommon or irreversible action, and state the
   real consequence. Whether deleting a PVC deletes data depends on its reclaim policy; say what
   is known.
7. **Empty states say what would appear and how to get it**, with a button or link where one
   exists; never a bare "None." (see "Empty states"). Nothing the reader needs may live only in
   text that disappears once the list fills.
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

### Empty states

An empty state is temporary, and the reader meets it before they know the feature; it is the
place to say what the feature is for and how to start [A1][N1].

- **One empty state per view, not one per category.** A section with nothing in it is hidden
  when another section on the page has content; when the whole view is empty, one empty state
  replaces all of its sections. No titled card whose body is "None." / 「沒有。」.
- **Title: what is missing, in the reader's words**: "No environment variables yet",
  「api 還沒有公開網域」. Description, when it adds something: one sentence on why one would
  want it or why there is none. Action: the verb that fixes it ("Add variable").
- **Say when nothing is needed.** A worker that only makes outgoing connections needs no
  address; a scheduled job receives no traffic. Say so instead of offering an action that does
  not apply.
- **A result is not an empty state.** "No warnings in the last hour." reports a check that
  passed; keep it as a line of text, without an illustration.
- Use the design system's `Empty` (`size="inline"` inside a section, the default size for a whole
  page) so every empty state has the same shape.

### Section titles and terms

- **A section title names its content** as the reader would ask for it ("Public domains",
  "Private network"), not its position or source in Zeabur ("Set here", "From other objects")
  and not a bare adjective (「公開」). Titles read as an outline of the page [F1].
- **One always-visible sentence under a title** says what the section holds when the term is new
  to a beginner. It stays when the section fills, so it is not lost with the empty state [A1].
- **Plain term first, Kubernetes term behind a disclosure.** The plain term is the label; the
  exact kinds and fields of a section go under one 「給 Kubernetes 使用者的細節」 disclosure at
  its end (`KubeDetails`), never as kind tags (`Ingress`, `Service`, `ClusterIP`,
  `PersistentVolumeClaim`) beside titles, rows or values. A kind shows by default only when the
  reader must act on that object (the command or YAML they are told to apply) or when the plain
  word alone is ambiguous, and then once per section. A section has at most one disclosure. Do
  not invent a middle word that is neither (「內部位址」); a professional searches for the kind,
  a beginner for the plain word [M2][N2][U1]. A tooltip never holds the only copy of something
  the reader must act on [N3].
- **Explain a term in one short sentence where the reader first meets it**, then use it bare.
- **Show the real value** (`api.shop.svc.cluster.local:8080`, with copy) rather than describe
  it; when there is none, say how to get one.

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
| Empty state titles | Sentence case, no period: `No volumes yet` | No final 。 |
| One-line notes, empty state descriptions | A sentence, ending with a period | Ends with 。 |
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
| project | 專案 | 项目 | A project card or row names the project; its namespace, when the name differs, is in the Kubernetes details (Kubernetes Namespace) |
| service | 服務 | 服务 | |
| cluster | 叢集 | 集群 | |
| node | 節點 | 节点 | |
| namespace | Namespace | Namespace | Kubernetes kind, as the dashboard writes it. Only where the namespace is what's chosen (use an existing namespace); a name kept by Kubernetes is a name, not a namespace |
| replica | 副本 | 副本 | Also where a node's trouble stops or blocks them (kubelet stopping pods); Pod stays for the cluster's own counts and the pod network. The replicas list's Kubernetes details say Pod |
| instance (a database's server: CNPG `spec.instances`, a replica set's `size`) | 執行個體 | 实例 | Never 成員, which names a person on the Members page |
| container | 容器 | 容器 | |
| image | 映像檔 | 镜像 | |
| volume | 磁碟區 | 卷 | The dashboard also says 硬碟／硬盘; a claim is not a disk. A sentence says 磁碟區 with the claim's name in parentheses, never "claim"; the PersistentVolume behind it is named as one |
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
| conditions (an object's `status.conditions`) | 狀況 | 状况 | As the Kubernetes docs translate it |
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
| use admin rights for a few minutes (the offer to elevate) | 暫時使用管理員權限 | 临时使用管理员权限 | Never 以管理員身分檢視: it changes rights, not the view. The time is Zeabur Identity's setting, so no number is promised |
| Admin / Developer / Viewer (a role in a project) | 管理員／開發者／檢視者 | 管理员／开发者／查看者 | |
| session | 工作階段 | 会话 | |
| credential | 認證資訊 | 凭证 | zh-TW 憑證 is a certificate |
| certificate | 憑證 | 证书 | |
| token | 權杖 | 令牌 | |
| passkey | 通行密鑰 | 通行密钥 | Apple's name in the system dialog that asks for it (Google's zh-TW wording differs; not verified); chosen over the dashboard's Latin "Passkey" so the page and that dialog match |
| sign in / sign out | 登入／登出 | 登录／退出登录 | |
| deploy, roll out | 部署 | 部署 | |
| auto-deploy new builds | 新建置完成後自動部署 | 新构建完成后自动部署 | Keel is named only when it is missing, when its own policy decides, and in the Kubernetes details |
| releases (a service's revisions) | 發布紀錄 | 发布记录 | |
| revision | 修訂版本 | 修订版本 | |
| restart | 重新啟動 | 重启 | |
| roll back | 回滾 | 回滚 | |
| apply a change | 套用 | 应用 | |
| review changes (a plan, as the API and docs keep calling it) | 檢查變更 | 检查变更 | Never 計畫／计划 on screen: it reads as a schedule. Expired: 這份變更已過期 |
| run a command | 執行 | 运行 | |
| use an existing namespace (show it as a project) | 使用既有的 Namespace | 使用已有的 Namespace | A namespace's own row says 使用這個 Namespace; the API and zeaburctl keep adopt |
| remove from Zeabur | 移除 | 移除 | |
| read-only | 唯讀 | 只读 | |
| view (compact action) | 檢視 | 查看 | |
| jobs (Jobs and CronJobs together) | 批次工作 | 批处理任务 | One-off Jobs are not scheduled |
| scheduled job | 排程工作 | 定时任务 | A CronJob |
| restore (from a backup) | 還原 | 还原 | Never 恢復／恢复, which is for resume and recover |
| PodDisruptionBudget | PodDisruptionBudget | PodDisruptionBudget | Kubernetes kind; no settled translation, so keep it |
| schedule (when a backup runs) | 排程 | 定时 | In words with its time zone (每天 03:00（UTC）); the cron expression on hover. Never 计划, which reads as a plan |
| background service (no public domain; the map's lane and the sidebar's group) | 背景服務 | 后台服务 | A CronJob in it is marked 排程／定时 |
| health check | 健康檢查 | 健康检查 | |
| traffic check (readiness) | 就緒檢查 | 就绪检查 | |
| restart check (liveness) | 存活檢查 | 存活检查 | |
| light / dark theme | 淺色／深色 | 亮色／暗色 | |
| Overview / Logs / Deployments / Networking / Variables / Settings (a service's tabs) | 概覽／日誌／部署紀錄／網路／環境變數／設定 | 概览／日志／部署记录／网络／变量／设置 | The dashboard's tab names; Deployments holds the releases and what changed |
| reachable / not reachable / not verified (a domain or address) | 可連線／無法連線／無法確認 | 可访问／无法访问／无法确认 | "Not verified" is never a failure: Zeabur could not check it from where it runs |
| connection check (one hop of the way to an address) | 連線檢查 | 连接检查 | |
| configured connection / guessed from variables (where a line on the map comes from) | 已設定的連線／從變數推測 | 已配置的连接／从变量推测 | Configured: an Ingress or a shared Secret or ConfigMap; guessed: a variable's value names the Service |
| waiting for web to be ready (an address whose only failing hop is a cause said above) | 等待 web 就緒 | 等待 web 就绪 | Neutral, never red; the cause is said once |
| this also means (what one cause also takes down) | 連帶影響 | 连带影响 | Inside the cause, never a card of its own |
| deployment stuck | 部署卡住 | 部署卡住 | A rollout that never finishes by itself |
| retrying (an event's `BackOff`) | 重試等待中 | 重试等待中 | An event's reason follows its plain word in a `Term` |
| check failed (an event's `Unhealthy`) | 檢查未通過 | 检查未通过 | |
| suggestion (a gap to close) | 建議 | 建议 | |
| errors in the app / settings to change / caused by the cluster (where a problem comes from, as a group title) | 應用程式出錯／需要調整設定／叢集造成的問題 | 应用出错／需要调整配置／集群造成的问题 | Group titles on Home; beside a single problem, a cause outside the service is a sentence, and the reader's own is not said |
| this needs a cluster administrator (who must act, when it is not the reader) | 這需要叢集管理員處理 | 这需要集群管理员处理 | Leads the fix, with 複製診斷 as the primary action; never said when the reader can fix it |
| public domain | 公開網域 | 公开域名 | An Ingress or HTTPRoute host; the dashboard's 公有網路 names the network, not the address. Ingress and HTTPRoute appear only in the Kubernetes details, YAML and suggestions about those objects; never "public URL" |
| private network | 私有網路 | 私有网络 | Reaching a service from inside the cluster through its `Service`; the dashboard says 內網 |
| private address | 私有位址 | 私有地址 | A `Service` DNS name and port; never 內部位址. Where the object itself is meant, write Kubernetes Service, since a bare Service reads as 服務 |
| private service (reachable only inside the cluster) | 私有服務 | 私有服务 | Replaces "internal" (內部) as a role |
| connection rules | 連線規則 | 连接规则 | `NetworkPolicy`; 政策 stays for policies in general |
| sends to (a route's backend) | 轉送到 | 转发到 | |
| storage class | 儲存類別 | 存储类 | `StorageClass` |
| debug container (a shell added beside a container that has none) | 除錯容器 | 调试容器 | An ephemeral container; it stays until the replica is replaced |
| kubectl terminal (a shell with kubectl and helm in the cluster) | kubectl 終端機 | kubectl 终端 | Runs as the signed-in user |
| node terminal (a root shell on a node's host); terminal pod (the privileged pod it runs in) | 節點終端機；終端機 Pod | 节点终端；终端 Pod | For cluster administrators only. Say plainly that it is full root on the host and bypasses Zeabur |
| scratch space (an `emptyDir`) | 暫存空間 | 临时空间 | Emptied when the replica is replaced |
| Strict (a service's security; the project allowing only Strict services) | 嚴格 | 严格 | Pod Security `restricted`: runs as non-root, no privilege escalation. The choice reads 嚴格（建議）／严格（推荐）; the level is in the Kubernetes details |
| Compatible (security that lets a container run as root) | 相容 | 兼容 | Pod Security `baseline`; the project enforcing it "allows Compatible services" |
| Details for Kubernetes users (the one disclosure of a section) | 給 Kubernetes 使用者的細節 | 给 Kubernetes 用户的细节 | Holds the section's kinds and fields |

## Server sentences

Diagnoses, gaps, traces and incidents (`diagnosis.*`, `gap.*`, `trace.*`, `incident.*`) arrive
from the API as a key and parameters, not prose. Their words live in
`crates/zeabur-gaps/messages/{en,zh-TW,zh-CN}.json`, one template per key, so the web and any
other client (`zeabur_gaps::messages::render`) word them the same way. The web reads them from
`web/src/i18n/generated/`, which `web/scripts/gen-messages.ts` writes; never edit those files.

To change a sentence, edit its template in all three catalogs, then run
`bun scripts/gen-messages.ts` in `web/` (`bun run dev` and `bun run build` also do it) and commit
both. `scripts/check-messages.sh` fails when the generated files are out of date, and
`cargo test -p zeabur-gaps` when a key the fixtures send has no words in a language. When a
sentence's output changes on purpose, update `messages/examples.json`, which both renderers are
tested against. A new key needs a template in each catalog and a fixture that sends it.

A template is text with placeholders, each `${name}` or `${name, kind, …}` with cases in braces:

| Placeholder | Writes |
|---|---|
| `${name}` | the parameter; nothing when it is absent |
| `${name, select, a {…} none {…} other {…}}` | the case named by the value; `none` when it is absent or empty, else `other` |
| `${name, plural, one {# item} other {# items}}` | `one` when the value is 1; `#` is the value |
| `${name, items, one {…} other {…}}` | `one` when the comma-separated value has one item |
| `${name, first, a {…} other {…}}` | the case named by the first item |
| `${name, list}` | the items joined with the language's `listSeparator` |
| `${name, list, and, a {…} b {…}}` | each item's case, the last joined with `listAnd`; items with no case are left out |
| `${name, duration}` | seconds in words, whole minutes as minutes (the catalog's `format`) |

Cases nest. Text between placeholders is written as it is, so spacing and punctuation follow
the locale's rules above.

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
- [F1] Microsoft, "Content design," *Fluent 2 Design System*. https://fluent2.microsoft.design/content-design
- [G2] Google, "Highlights," *Google developer documentation style guide*. https://developers.google.com/style/highlights
- [M1] Microsoft, "Top 10 tips for Microsoft style and voice." https://learn.microsoft.com/en-us/style-guide/top-10-tips-style-voice
- [M2] Microsoft, "Use technical terms carefully," *Microsoft Writing Style Guide*. https://learn.microsoft.com/en-us/style-guide/word-choice/use-technical-terms-carefully
- [M4] Microsoft, *Chinese (Traditional) Localization Style Guide*, 2024. https://download.microsoft.com/download/2/9/3/293e6d41-ba40-451b-a41e-94bdb6242ae3/zho-twn-StyleGuide.pdf
- [M5] Microsoft, *Chinese (Simplified) Localization Style Guide*, 2024. https://download.microsoft.com/download/1/5/9/159cb91c-b61b-4385-97ca-80ccc7ff1fa0/zho-chn-StyleGuide.pdf
- [N1] "Designing Empty States in Complex Applications," Nielsen Norman Group. https://www.nngroup.com/articles/empty-state-interface-design/
- [N2] "Plain Language Is for Everyone, Even Experts," Nielsen Norman Group. https://www.nngroup.com/articles/plain-language-experts/
- [N3] "Tooltip Guidelines," Nielsen Norman Group. https://www.nngroup.com/articles/tooltip-guidelines/
- [R3] A. Reinhart et al., "Do LLMs write like humans?," *PNAS* 122(8), 2025. https://pmc.ncbi.nlm.nih.gov/articles/PMC11874169/
- [R5] WikiProject AI Cleanup, "Wikipedia:Signs of AI writing." https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing
- [U1] GOV.UK, "Clear language," *Writing to GOV.UK standards*. https://guidance.publishing.service.gov.uk/writing-to-gov-uk-standards/writing-guidelines/clear-language/
