# Wording

Zeabur's own wording: the term chosen for each concept, how each kind of interface element reads,
and the rules that come from Zeabur's screens. It covers `web/src/i18n/` (`en.ts` defines the keys;
`zh-TW.ts` and `zh-CN.ts` are typed against it), the English messages the API sends with a
problem, the sign-in pages `zeabur` serves, `zeaburctl`'s output and the docs.

The general rules for English, zh-TW and zh-CN (sentence order, plain words, punctuation, spacing,
AI tells) are in the `zeabur-writing` skill, `.claude/skills/zeabur-writing/`. Where the two
differ, a term or convention here wins.

## Zeabur's rules

1. **Alert titles describe the situation; buttons name the result.** Never "Error" as a title;
   "Delete disk" / "Cancel", not "OK" / "Yes". Body text only when it adds a consequence; do not
   explain the buttons. The button that applies a review follows one rule, whether a form, YAML,
   a drawer or the draft asked: one kind of change takes its verb ("Restart 3 services", "Save
   variables"); changes that only create say "Create", with the count when there are more than
   one ("Create 3 objects"); several kinds together say "Apply 4 changes". A form whose objects
   are only the means (database backups) says the change in its own words and takes its own verb
   ("Set up backups"), not the object edits.
2. **Limits where they bite.** Warn only before an uncommon or irreversible action, and state the
   real consequence. Whether deleting a disk deletes data depends on its reclaim policy; say what
   is known.
3. **Empty states say what would appear and how to get it**, with a button or link where one
   exists; never a bare "None." (see "Empty states"). Nothing the reader needs may live only in
   text that disappears once the list fills.
4. **A setting's label says what "on" does.** Link to a setting rather than describe where it is.
5. **One term per concept**, from the term list below. Four kinds people meet every day have a
   plain word and use it everywhere on screen: Pod is *replica* (副本), PersistentVolumeClaim is
   *disk* (磁碟), Secret is *secret* (機密變數), ConfigMap is *config file* (設定檔). The kind
   name appears only where a reader needs it for `kubectl`: in YAML, on the Objects page
   and as a secondary label beside the plain word (the Objects page, an object's title), never
   in a heading or a sentence. Other kinds and fields (Deployment, selector, `replicas`) stay as
   they are and may repeat. Interpolations, numbers, defaults and code stay exact.
6. **Errors never echo a secret**, and they keep the raw cause (details, copy) out of the
   headline.

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
- **Plain term in the section, exact objects in YAML.** The plain term is the label. A section
  has no Kubernetes-details disclosure and no kind tags (`Ingress`, `Service`, `ClusterIP`,
  `PersistentVolumeClaim`) beside titles, rows or values. The exact kinds and fields live in a
  plan's YAML tab and on the Objects page. A fact the reader acts on is said in plain words in
  the section, as a sentence or a row: a volume's data stays or goes with it, the Zeabur Catalog
  version. A kind shows only when the reader must act on that object (the command or YAML they
  are told to apply) or when the plain word alone is ambiguous, and then once per section. Do
  not invent a middle word that is neither (「內部位址」); a professional searches for the kind,
  a beginner for the plain word [M2][N2][U1]. A tooltip never holds the only copy of something
  the reader must act on [N3].
- **Explain a term in one short sentence where the reader first meets it**, then use it bare.
- **Show the real value** (`api.shop.svc.cluster.local:8080`, with copy) rather than describe
  it; when there is none, say how to get one.

## Chinese

- `zh-TW.ts` and `zh-CN.ts` are two editorial passes, not a character conversion.
- Use `common.colon` instead of a literal colon. A placeholder that may hold Chinese or Latin
  text takes its space from `latinStart` / `latinEnd` (`web/src/i18n/zh-TW.ts`), never a
  hard-coded one.
- `scripts/check-zh-tw.sh` checks zh-TW interface vocabulary and punctuation;
  Vale (`.vale.ini`) checks docs and all three catalogues. Take a suggestion only when it
  fits the technical meaning.

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

- **Zeabur** is the product, always capitalized. Its parts are **Zeabur Identity** (sign-in) and
  **Zeabur Catalog** (tested component versions and how to install them). In Chinese the names
  stay in Latin letters.
- A *service* is what the reader deploys (服務／服务); `Service` is the Kubernetes kind. A
  *project* is a namespace Zeabur shows as one (專案／项目). A *replica* is a pod of a service
  (副本), and so is every other Pod, the cluster's own counts included.
- Other Kubernetes kinds stay in Latin letters in Chinese, Namespace included, as Zeabur's hosted
  dashboard writes it; in English running text they are lower case unless the kind itself is
  meant (`the Namespace shop`). The four kinds of rule 5 are the exception;
  `web/src/i18n/kind-name.ts` gives the plain word for a kind in code.

## Terms

One choice per concept. Where Zeabur's hosted dashboard names the same thing on screen, this
interface uses its word unless it breaks a rule above; the last column says where it differs and
why. Add a row before introducing a new concept.

| English | zh-TW | zh-CN | Note |
|---|---|---|---|
| project | 專案 | 项目 | A project card or row names the project; its namespace, when the name differs, is on the Objects page |
| service | 服務 | 服务 | |
| cluster | 叢集 | 集群 | |
| node | 節點 | 节点 | |
| namespace | Namespace | Namespace | Kubernetes kind, as the dashboard writes it. Only where the namespace is what's chosen (use an existing namespace); a name kept by Kubernetes is a name, not a namespace |
| replica (a Pod) | 副本 | 副本 | Everywhere: a node's counts, the replica network, a terminal replica (終端機副本). YAML and the Objects page say Pod |
| instance (a database's server: CNPG `spec.instances`, a replica set's `size`) | 執行個體 | 实例 | Never 成員, which names a person on the Members page |
| container | 容器 | 容器 | |
| image | 映像檔 | 镜像 | |
| volume | 磁碟區 | 卷 | What a service mounts. A sentence says 磁碟區 with the disk's name in parentheses, never "claim"; the PersistentVolume behind it is named as one |
| port | 連接埠 | 端口 | |
| domain | 網域 | 域名 | |
| environment variable; Variables tab | 環境變數 | 环境变量; tab 变量 | Tab names as on the dashboard |
| Usage tab, resource usage | 用量 | 用量 | Dashboard's Usage tab |
| Overview | 概覽 | 概览 | |
| Settings | 設定 | 设置 | |
| General (settings section) | 一般設定 | 通用设置 | |
| Description | 描述 | 描述 | |
| Danger zone | 危險區域 | 危险区域 | |
| secret (a Secret) | 機密變數 | 机密变量 | 密钥 is a key. The S3 "secret access key" keeps its name |
| config file (a ConfigMap) | 設定檔 | 配置文件 | |
| disk (a PersistentVolumeClaim) | 磁碟 | 磁盘 | The claim a volume is made from. What a service mounts is a volume (磁碟區／卷). The PersistentVolume behind it is named as one |
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
| admin account (passkey only, full rights; what the Admin role gives) | 管理員帳號 | 管理员账号 | Not 管理者帳戶 |
| regular account (every sign-in method, the rights its roles give) | 一般帳號 | 普通账号 | |
| device-bound passkey (BE unset: a security key, or a passkey that stays on one device) | 不會同步的通行密鑰 | 不会同步的通行密钥 | Say what it does, not 綁定裝置; synced ones are 會同步的通行密鑰 |
| sign-in settings (teams, AAL3) | 登入設定 | 登录设置 | |
| admin idle sign-out (the `teams` setting: 1 hour / 15 minutes) | 管理員閒置多久登出 | 管理员闲置多久退出登录 | Name what it controls; the options are the times, not team counts |
| Admin / Developer / Viewer (a role in a project) | 管理員／開發者／檢視者 | 管理员／开发者／查看者 | |
| session | 工作階段 | 会话 | |
| credential | 認證資訊 | 凭证 | zh-TW 憑證 is a certificate |
| certificate | 憑證 | 证书 | |
| token | 權杖 | 令牌 | |
| passkey | 通行密鑰 | 通行密钥 | Apple's name in the system dialog that asks for it; chosen over the dashboard's Latin "Passkey" so the page and that dialog match |
| sign in / sign out | 登入／登出 | 登录／退出登录 | |
| deploy, roll out | 部署 | 部署 | |
| auto-deploy new builds | 新建置完成後自動部署 | 新构建完成后自动部署 | Keel is named only when it is missing, when its own policy decides, and in YAML |
| releases (a service's revisions) | 發布紀錄 | 发布记录 | |
| revision | 修訂版本 | 修订版本 | |
| restart | 重新啟動 | 重启 | |
| roll back | 回滾 | 回滚 | |
| apply a change | 套用 | 应用 | |
| review changes (a plan, as the API and docs keep calling it) | 檢查變更 | 检查变更 | Never 計畫／计划 on screen: it reads as a schedule. Expired: 這份變更已過期 |
| installer plan (what `zeabur-installer plan` prints; the review's Full plan) | 安裝步驟 | 安装步骤 | As the installer's own 完整安裝步驟／完整安装步骤; never 計畫／计划. The command itself stays `plan` <!-- writing-allow: zh-punct (English notes quote Chinese terms) --> |
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
| public domain | 公開網域 | 公开域名 | An Ingress or HTTPRoute host; the dashboard's 公有網路 names the network, not the address. Ingress and HTTPRoute appear only in YAML, on the Objects page and in suggestions about those objects; never "public URL" |
| private network | 私有網路 | 私有网络 | Reaching a service from inside the cluster through its `Service`; the dashboard says 內網 |
| private address | 私有位址 | 私有地址 | A `Service` DNS name and port; never 內部位址. Where the object itself is meant, write Kubernetes Service, since a bare Service reads as 服務 |
| private service (reachable only inside the cluster) | 私有服務 | 私有服务 | Replaces "internal" (內部) as a role |
| connection rules | 連線規則 | 连接规则 | `NetworkPolicy`; 政策 stays for policies in general |
| sends to (a route's backend) | 轉送到 | 转发到 | |
| storage class | 儲存類別 | 存储类 | `StorageClass` |
| debug container (a shell added beside a container that has none) | 除錯容器 | 调试容器 | An ephemeral container; it stays until the replica is replaced |
| kubectl terminal (a shell with kubectl and helm in the cluster) | kubectl 終端機 | kubectl 终端 | Runs as the signed-in user |
| node terminal (a root shell on a node's host); terminal replica (the privileged replica it runs in) | 節點終端機；終端機副本 | 节点终端；终端副本 | For cluster administrators only. Say plainly that it is full root on the host and bypasses Zeabur |
| scratch space (an `emptyDir`) | 暫存空間 | 临时空间 | Emptied when the replica is replaced |
| Strict (a service's security; the project allowing only Strict services) | 嚴格 | 严格 | Pod Security `restricted`: runs as non-root, no privilege escalation. Zeabur picks it for an image whose user is a numeric UID other than 0; the level is in the plan's YAML |
| Compatible (security that lets a container run as root) | 相容 | 兼容 | Pod Security `baseline`, which a new project enforces |
| raw error (the folded raw answer under an error or a diagnosis) | 原始錯誤 | 原始错误 | Holds the API's own code, reason and message, the replica and exit code, and the link to the problem type's page. Sections have no Kubernetes-details disclosure |

## Server sentences

Diagnoses, gaps, traces and incidents (`diagnosis.*`, `gap.*`, `trace.*`, `incident.*`) arrive
from the API as a key and parameters, not prose. Their words are TypeScript functions in
`web/src/i18n/server/{en,zh-TW,zh-CN}.ts`, one per key, named `sentence.<key>` and taking the
parameters as strings. Problem sentences (`sentence.problem.*`, `sentence.members.*`,
`sentence.account.*`, `sentence.page.*`) are written the same way in the three dictionaries. The
server never renders them.

To change a sentence, edit its function in all three files; `tsc` fails when a language lacks a
key. A new key needs a function in each file and a fixture or code in `zeabur-modules` that sends
it; `bun test` fails when a key those crates send has no words. A parameter that may be absent
is read as `p.name ?? ""`. Test a branch that is easy to get wrong in `web/src/i18n/server.test.ts`.

## Review

Read changed text on the rendered page, in every locale, at desktop and narrow widths: wrapping,
truncation, mixed CJK and Latin, buttons that overflow. Check that the three locales state the
same facts, limits and conditions. `bun run typecheck`, `bun run test`,
`vale` and `scripts/check-zh-tw.sh` must pass; a passing check is not the
review.

## Sources

- [A1] Apple, "Writing," *Human Interface Guidelines*. https://developer.apple.com/design/human-interface-guidelines/writing
- [F1] Microsoft, "Content design," *Fluent 2 Design System*. https://fluent2.microsoft.design/content-design
- [M2] Microsoft, "Use technical terms carefully," *Microsoft Writing Style Guide*. https://learn.microsoft.com/en-us/style-guide/word-choice/use-technical-terms-carefully
- [N1] "Designing Empty States in Complex Applications," Nielsen Norman Group. https://www.nngroup.com/articles/empty-state-interface-design/
- [N2] "Plain Language Is for Everyone, Even Experts," Nielsen Norman Group. https://www.nngroup.com/articles/plain-language-experts/
- [N3] "Tooltip Guidelines," Nielsen Norman Group. https://www.nngroup.com/articles/tooltip-guidelines/
- [U1] GOV.UK, "Clear language," *Writing to GOV.UK standards*. https://guidance.publishing.service.gov.uk/writing-to-gov-uk-standards/writing-guidelines/clear-language/
