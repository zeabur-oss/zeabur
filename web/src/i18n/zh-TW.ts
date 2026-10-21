import type { Messages } from "@/i18n/en"
import { cuts, listZh, probeTiming, thingZh, type P, type ProbeTiming } from "@/i18n/sentence-parts"

// A space where Han text meets a Latin word ("檢視 Secrets 需要…").

/** How many replicas of the release before still serve, after a stuck rollout's sentence. */
const servingZhTW = (p: P) =>
  Number(p.serving ?? 0) > 0 ? `上一個版本的 ${p.serving} 個副本仍在提供服務。` : ""

const latinStart = (text: string) => (/^[A-Za-z0-9]/.test(text) ? " " : "")
const latinEnd = (text: string) => (/[A-Za-z0-9]$/.test(text) ? " " : "")

/** What a refused verb does, in a phrase: `patch` changes, `create` adds. */
const verbZhTW: Record<string, string> = {
  get: "讀取",
  list: "列出",
  watch: "監看",
  create: "新增",
  update: "變更",
  patch: "變更",
  delete: "刪除",
}

const objectZhTW = (p: P) => (p.object === "Secret" ? `機密 ${p.name}` : `設定檔 ${p.name}`)

/** Who a volume's mount failure stops: the container, else the replica. */
const mountedZhTW = (p: P) => p.container ?? `副本 ${p.pod}`

/** Seconds in words, whole minutes as minutes. */
const secondsZh = (s: number) => (s >= 60 && s % 60 === 0 ? `${s / 60} 分鐘` : `${s} 秒`)

/** A wait in its largest unit, rounded up: 90 s is 2 minutes. */
const waitZh = (s: number) =>
  s < 60 ? `${s} 秒` : s < 3600 ? `${Math.ceil(s / 60)} 分鐘` : `${Math.ceil(s / 3600)} 小時`

/** The sentence, led by "by default" when both values are the defaults, else followed by which one is. */
const timingZhTW = (t: ProbeTiming, sentence: string) =>
  t.defaults
    ? `預設情況下，${sentence}`
    : `${sentence}${t.periodOnly ? `每 ${secondsZh(t.period)}是預設值。` : ""}${
        t.thresholdOnly ? `${t.threshold} 次是預設值。` : ""
      }`

export const zhTW: Messages = {
  "shell.skip": "跳到主要內容",
  "shell.nav": "主選單",
  "shell.home": "首頁",
  "shell.projects": "專案",
  "shell.settings": "設定",
  "shell.cluster": "叢集",
  "shell.search": "搜尋",
  "shell.jumpTo": "前往…",
  "shell.menu": "選單",
  "shell.findProject": "尋找專案",
  "shell.manage": "管理",
  "shell.whoamiUnknown": "不明使用者",
  "shell.signOut": "登出",
  "shell.theme": "主題",
  "elevate.retry": "以管理員身分重試",
  "elevate.title": "管理員權限",
  "elevate.waiting": "在新視窗用通行密鑰確認。",
  "elevate.notAllowed": "這個 Zeabur 不能要求管理員權限，請洽叢集管理員。",
  "elevate.blocked": "瀏覽器擋下了視窗。允許這個網站開啟彈出式視窗後，再試一次。",
  "elevate.failed": "沒有取得管理員權限。",
  "elevate.signedOut": "工作階段已結束，重新登入後再繼續。",
  "elevate.signIn": "登入",
  "elevate.until": (time) => `管理員權限至 ${time}`,
  "elevate.activeDetail": "在此之前，你的所有操作都會使用管理員權限。",
  "elevate.drop": "放棄管理員權限",
  "elevate.use": "使用管理員權限",
  "shell.switchProject": "切換專案",
  "column.label": "服務",
  "column.other": "其他物件",
  "column.advanced": "進階",
  "column.namespace": "Namespace",

  "theme.system": "跟隨系統",
  "theme.light": "淺色",
  "theme.dark": "深色",

  "common.colon": "：",
  "common.loading": "正在載入…",
  "common.retry": "再試一次",
  "common.all": "全部",
  "common.close": "關閉",
  "common.copy": "複製",
  "common.copied": "已複製",
  "common.status": "狀態",
  "common.sep": "，",
  "common.list": (items) => listZh("zh-TW", items),
  "common.domains": "網域",
  "common.newTab": "（在新分頁開啟）",
  "common.notFound": "找不到這個頁面",
  "common.backToProjects": "返回專案",
  "common.managedBy": (tool) => `由 ${tool} 管理`,
  "common.managedByDetail": (tool) => `${tool} 下次同步或升級時，會覆寫所有不是透過它做的變更。`,

  "problem.whatToDo": "怎麼處理",
  "problem.kubernetes": "給 Kubernetes 使用者的細節",
  "problem.code": "HTTP 狀態碼",
  "problem.reason": "原因",
  "problem.message": "訊息",
  "problem.title.bad-request": "請求無效",
  "problem.title.unauthenticated": "你尚未登入",
  "problem.title.signed-out": "你已登出",
  "problem.title.forbidden": "你沒有權限",
  "problem.title.elevation-required": "這需要管理員權限",
  "problem.title.quota-exceeded": "這會超過專案的用量上限",
  "problem.title.policy-denied": "叢集裡的政策拒絕了這項變更",
  "problem.title.not-found": "找不到這個物件",
  "problem.title.log-unavailable": "無法取得日誌",
  "problem.title.conflict": "這段期間已有其他變更",
  "problem.title.managed": "由其他工具管理",
  "problem.title.plan-expired": "計畫已過期",
  "problem.title.system-namespace": "這是 Kubernetes 自用的 Namespace",
  "problem.title.invalid": "叢集拒絕了這項變更",
  "problem.title.too-many-requests": "叢集忙碌中",
  "problem.title.unavailable": "無法連上叢集",
  "problem.title.internal": "Zeabur 發生錯誤",
  "problem.fix.bad-request": "檢查請求後再試一次。",
  "problem.fix.unauthenticated": "重新登入。",
  "problem.fix.signed-out": "重新登入。",
  "problem.fix.forbidden": "向叢集管理員要求權限，或改用有權限的帳號。",
  "problem.fix.elevation-required": "以管理員身分重試，或向叢集管理員要求權限。",
  "problem.fix.quota-exceeded": "減少要求的量、釋出專案已用的資源，或請叢集管理員調高上限。",
  "problem.fix.policy-denied": "依政策指出的地方修改，或向叢集管理員詢問這項政策。",
  "problem.fix.not-found": "重新載入頁面，看看叢集目前有什麼。",
  "problem.fix.log-unavailable": "改看這次執行的日誌，或稍候再重新整理。",
  "problem.fix.conflict": "重新載入看最新版本，再做一次變更。",
  "problem.fix.managed":
    "到管理工具讀取設定的地方變更，例如 Helm values 或 Git 儲存庫。這裡仍然可以重新啟動。",
  "problem.fix.plan-expired": "重新產生計畫。",
  "problem.fix.system-namespace": "改為納入你自己的 Namespace。",
  "problem.fix.invalid": "修正細節中叢集指出的欄位，再試一次。",
  "problem.fix.too-many-requests": "稍等一下再試一次。",
  "problem.fix.unavailable": "確認叢集正在運作，而且這台機器連得到它。",
  "problem.fix.internal": "再試一次。如果一直發生，請附上細節回報。",
  "problem.detail": "Zeabur 的訊息",
  "problem.about": "這類問題的說明",
  "problem.retryAfter": (s) => `叢集要求等 ${waitZh(s)}。`,
  "problemPage.label": "問題類型",
  "problemPage.rule": "規則",
  "problemPage.kubernetes": "Kubernetes 文件",
  "problemPage.rule.bad-request":
    "Zeabur 會在請求送到叢集之前先檢查：要變更的內容必須完整、格式正確。不符合的請求，Zeabur 會直接拒絕，什麼都不寫入。",
  "problemPage.rule.unauthenticated":
    "每個請求都以你自己的登入身分送到叢集。如果叢集不再接受這個登入（已過期或已撤銷），在你重新登入之前，什麼都無法讀取或變更。",
  "problemPage.rule.signed-out":
    "你的 Zeabur 工作階段已結束，Zeabur 手上沒有可以代你使用的登入。在你重新登入之前，什麼都不會讀取或變更。",
  "problemPage.rule.forbidden":
    "叢集用規則（RBAC）決定每個人能做什麼：誰可以讀取或變更哪些種類的物件、在哪些專案裡。Zeabur 只用你自己的權限行事，你的權限不允許的事，它也無法代你做。",
  "problemPage.rule.elevation-required":
    "你平常的權限不允許這件事，但這個 Zeabur 可以在你用通行密鑰確認後，短時間給你管理員權限。在權限到期或你放棄之前，你的所有操作都會使用它。",
  "problemPage.rule.quota-exceeded":
    "專案可以設定所有服務加起來能保留的上限：CPU、記憶體、副本或物件的數量。變更如果會超過上限，叢集就會拒絕，與你的權限無關。",
  "problemPage.rule.policy-denied":
    "叢集可以用自己的政策（由 admission webhook 或政策引擎執行）檢查每一項變更。政策拒絕時，它的原話會說明原因；你的權限改變不了它的回應。",
  "problemPage.rule.not-found": "要找的東西不在叢集裡：頁面讀取之後，可能有人刪除或改名了它。",
  "problemPage.rule.log-unavailable":
    "叢集只保留容器這一次與上一次執行的日誌，而且只在副本還存在時保留。從未寫出的日誌，或副本已經不在的日誌，都無法讀取。",
  "problemPage.rule.conflict":
    "Zeabur 只會把變更寫到你預覽的那個版本上。如果之後有人或其他程式（例如部署工具、自動擴縮）改了這個物件，就什麼都不會寫入，誰的變更都不會遺失。",
  "problemPage.rule.managed":
    "這個物件由其他工具（Helm、Argo CD、Flux）管理。那個工具下一次同步或升級時，會換回自己的版本，蓋掉在這裡做的變更，所以 Zeabur 不接受。",
  "problemPage.rule.plan-expired":
    "計畫就是你預覽過、Zeabur 將要寫入的內容。它只在幾分鐘內有效，這樣套用時，內容仍符合叢集現況。",
  "problemPage.rule.system-namespace":
    "Kubernetes 保留了幾個 Namespace 給自己的元件使用（kube-system、kube-public、kube-node-lease），它們不能成為專案。",
  "problemPage.rule.invalid":
    "叢集儲存物件之前，會依照它的結構描述與規則檢查。叢集拒絕了這個物件，並指出哪些欄位無效。",
  "problemPage.rule.too-many-requests":
    "叢集會限制同時處理的請求數，以維持回應速度。叢集暫時拒絕了這個請求，沒有任何變更。",
  "problemPage.rule.unavailable":
    "Zeabur 連不到叢集的 API 伺服器：它可能停機或正在重新啟動，或從 Zeabur 執行的地方連不到。",
  "problemPage.rule.internal": "出錯的是 Zeabur 本身，不是叢集。詳細資料裡有 Zeabur 知道的資訊。",
  "refused.admin": "這需要管理員權限。",
  "refused.adminFor": (what) => `${what}${latinEnd(what)}需要管理員權限。`,
  "refused.rights": "這需要更多權限。",
  "refused.rightsFor": (what) => `${what}${latinEnd(what)}需要更多權限。`,
  "refused.ask": "請洽叢集管理員。",
  "refused.view": "以管理員身分檢視",
  "refused.details": "細節",
  "refused.missing": "缺少的權限",
  "refused.see": (what) => `檢視${latinStart(what)}${what}`,
  "refused.some": "部分內容",
  "refused.terminal": "開啟終端機",
  "refused.reveal": (name) => `顯示 ${name} 的值`,
  "refused.listThese": "列出這些物件",
  "refused.adopt": (ns) => `納入 ${ns}`,
  "refused.unadopt": (ns) => `從 Zeabur 移除 ${ns}`,

  "status.ready": "就緒",
  "status.rollingOut": "部署中",
  "status.failing": "異常",
  "status.degraded": "未全數就緒",
  "status.scheduled": "已排程",
  "status.paused": "已暫停",
  "status.rolloutStuck": "部署卡住",
  "status.failed": "失敗",
  "status.scaledToZero": "已縮為 0",
  "status.completed": "已完成",
  "status.unknown": "未知",
  "status.lastExit": (code, ago) => (ago ? `上次結束碼 ${code}，${ago}` : `上次結束碼 ${code}`),
  "reason.crashing": "反覆崩潰",
  "reason.runFailed": "執行失敗",
  "reason.outOfMemory": "記憶體不足",
  "reason.imagePull": "無法下載映像檔",
  "reason.imageName": "映像檔名稱有誤",
  "reason.configMissing": "缺少設定",
  "reason.cantStart": "無法啟動",
  "reason.waitingForRoom": "等待可用資源",
  "reason.starting": "正在啟動",
  "reason.evicted": "已移出節點",
  "reason.releaseStuck": "部署卡住",
  "reason.gaveUp": "已放棄重試",
  "phase.Running": "執行中",
  "phase.Progressing": "部署中",
  "phase.Degraded": "未全數就緒",
  "phase.Paused": "已暫停",
  "phase.Suspended": "已停用",
  "phase.Completed": "已完成",
  "phase.Failed": "失敗",
  "phase.Unknown": "未知",

  "role.web": "網站",
  "role.internal": "私有服務",
  "role.worker": "背景工作",
  "role.job": "批次工作",
  "role.data": "資料",

  "metric.restarts": "重新啟動次數",
  "metric.warnings": "警告",
  "metric.services": "服務",

  "home.title": "首頁",
  "home.status": "狀態",
  "home.rollingOut": (n) => `${n} 個正在部署`,
  "home.nodesReady": (ready, total) => `${ready}/${total} 個節點就緒`,
  "home.allProjects": "所有專案",
  "home.warningsHour": "警告（1 小時）",
  "home.noEventsPermission": "你沒有列出叢集事件的權限。",
  "home.noServices": "還沒有服務",

  "cluster.title": "叢集",
  "cluster.tabs": "叢集頁面",
  "cluster.overview": "概覽",
  "cluster.catalogue": "目錄",
  "cluster.endOfLife": "已終止支援",
  "cluster.endOfLifeOn": (date) => `${date} 終止支援`,
  "cluster.skewBadge": "節點版本不受支援",
  "cluster.upgrading": "升級中",
  "cluster.available": (version) => `可升級至 ${version}`,
  "cluster.patch": (version) => `修補版本 ${version}`,
  "cluster.nextMinor": (version) => `下一個次版本 ${version}`,
  "cluster.allClear": "沒有需要處理的項目。",
  "cluster.more": (n) => `還有 ${n} 項`,
  "cluster.fewer": "收合",
  "cluster.attEolSoon": (minor, date) => `Kubernetes ${minor} 將於 ${date} 終止支援`,
  "cluster.attEolPast": (minor, date) => `Kubernetes ${minor} 已於 ${date} 終止支援`,
  "cluster.attEolOld": (minor) => `Kubernetes ${minor} 已終止支援`,
  "cluster.attSkew": (node, version) => `${node} 的 kubelet 為 ${version}，超出支援的版本差距`,
  "cluster.attTight": (node, what, percent) =>
    `${node}：${what}${latinEnd(what)}保留量已達 ${percent}`,
  "cluster.attNode": (node, state) => `${node}：${state}`,
  "cluster.attComponent": (name, version, tested) => `${name} ${version} 比已測試的 ${tested} 舊`,
  "cluster.noProjects": "還沒有專案",
  "cluster.noProjectsBody": "專案是 Zeabur 顯示的 Kubernetes Namespace。到「專案」頁面納入一個。",
  "cluster.openProjects": "開啟專案頁面",
  "cluster.nodes": "節點",
  "cluster.cpu": "CPU",
  "cluster.memory": "記憶體",
  "cluster.cores": (n) => `${n} 核`,
  "cluster.requestedOf": (requested, allocatable) => `已保留 ${requested}/${allocatable}`,
  "cluster.allocatable": (allocatable) => `可分配 ${allocatable}`,
  "cluster.used": (used) => `已使用 ${used}`,
  "cluster.limits": (limits) => `上限 ${limits}`,
  "cluster.podsOf": (placed, room) => `${placed}/${room} 個 Pod`,
  "cluster.pods": "Pod",
  "cluster.notReady": "未就緒",
  "cluster.cordoned": "已停止排程",
  "cluster.worker": "worker",
  "cluster.kubelet": "kubelet",
  "cluster.skewMixed": "與控制平面的次版本不同，仍受支援。",
  "cluster.skewUnsupported": "不受支援：比控制平面新，或比它舊超過 3 個次版本。",
  "cluster.noMetrics": "安裝 metrics-server 才能顯示用量。",
  "cluster.metricsUnavailable": "metrics-server 沒有回應。",
  "cluster.noUsageYet": "還沒有測得用量。",
  "cluster.partialPods": (n) => `保留量只計入前 ${n} 個 Pod。`,
  "cluster.unscheduled": (n) => `${n} 個 Pod 等待排程`,
  "cluster.warnings": "最近一小時的警告",
  "home.showWarnings": (n) => `顯示 ${n} 則警告`,
  "cluster.noWarnings": "最近一小時沒有警告。",
  "cluster.moreWarnings": (n) => `還有 ${n} 則`,
  "cluster.partialEvents": "只讀取了最新的 5,000 則事件。",
  "cluster.noComponents": "還沒有安裝目錄中的元件。從下方挑一個，看看怎麼安裝。",
  "cluster.tested": (version) => `已測試 ${version}`,
  "cluster.testedVersion": "已測試版本",
  "cluster.newerThanTested": "比已測試版本新",
  "cluster.sourceK3s": "k3s",
  "cluster.sourceHelmChart": "HelmChart",
  "cluster.sourceHelm": "Helm",
  "cluster.sourceOther": "—",
  "cluster.roleReadsSecrets":
    "這個角色能讀取叢集裡的每個 Secret，包括密碼、金鑰和權杖。只授予你願意讓他擔任叢集管理員的人。",
  "cluster.copyRole": "複製 ClusterRole",
  "cluster.whatProjects": "專案",
  "cluster.whatNodes": "節點",
  "cluster.whatRequests": "Pod 的保留量",
  "cluster.whatUpgrades": "升級計畫",
  "cluster.whatUsage": "節點用量",
  "cluster.whatWarnings": "叢集的警告",
  "cluster.whatComponents": "已安裝的元件",

  "projects.title": "專案",
  "projects.adopted": "專案",
  "projects.namespaceOnly": "Namespace",
  "projects.other": "其他 Namespace",
  "projects.noneBody": "專案是 Zeabur 顯示的 Kubernetes Namespace。從下方的 Namespace 納入一個。",
  "projects.showSystem": "顯示系統 Namespace",
  "projects.forbiddenTitle": "你沒有權限列出專案",
  "projects.view": "檢視方式",
  "projects.viewGrid": "卡片",
  "projects.viewList": "清單",

  "project.sections": "專案頁面",
  "project.entry.overview": "概覽",
  "project.services": "服務",
  "project.events": "事件",
  "project.noServices": "還沒有服務",
  "project.seeObjects": "檢視所有物件",
  "project.healthyOf": (healthy, total) => `${healthy}/${total} 正常`,

  "attention.title": "需要處理",
  "attention.ready": (ready, desired) => `${ready}/${desired} 就緒`,
  "attention.logsOf": (pod) => `${pod} 的日誌`,
  "attention.eventsOf": (pod) => `${pod} 的事件`,
  "overview.healthy": "正常",
  "overview.failingCount": (n) => `${n} 個失敗`,
  "overview.degradedCount": (n) => `${n} 個未就緒`,
  "events.show": "顯示事件",
  "events.warnings": "警告",
  "events.noWarnings": "沒有警告。",
  "events.empty": "最近沒有事件。Kubernetes 只保留大約一小時內的事件。",
  "events.label": "事件，最新的在前",
  "events.warning": "警告",
  "events.normal": "一般",
  "events.forPod": "副本",
  "events.clearPod": "顯示所有副本",
  "domains.https": "已設定 HTTPS",
  "resource.unused": "未使用",
  "resource.unusedCount": (n) => `${n} 個未使用`,
  "storage.requestedTotal": (size) => `共申請 ${size}`,
  "storage.notMounted": "沒有服務掛載",

  "table.service": "服務",
  "table.status": "狀態",
  "table.restarts": "重新啟動",
  "table.domain": "網域",
  "table.actions": "操作",
  "table.kind": "類型",
  "table.pods": "副本",
  "services.search": "篩選服務",
  "services.attention": "需要處理",
  "services.healthy": "正常",
  "services.allKinds": "所有類型",
  "services.count": (shown, total) => (shown === total ? `${total} 個服務` : `${shown} / ${total}`),
  "services.noMatch": "沒有符合的服務。",

  "lane.domains": "網域",
  "lane.web": "網站",
  "lane.internal": "私有服務",
  "lane.workers": "背景與批次工作",
  "lane.data": "資料",
  "edge.declared": "宣告",
  "edge.inferred": "推測",
  "edge.route": (ingress) => `Ingress ${ingress}`,
  "edge.shared": (kind, name) => `${kind} ${name}`,
  "edge.env": (variable) => variable,
  "canvas.zoomIn": "放大",
  "canvas.zoomOut": "縮小",
  "canvas.fit": "符合畫面大小",
  "canvas.legend": "連線說明",
  "canvas.legendHint":
    "根據 Kubernetes 物件和環境變數值畫出，不是根據實際流量。宣告：Ingress，或共用的 Secret、ConfigMap。推測：環境變數值提到了這個 Service。",
  "architecture.edges": (n) => `${n} 條連線`,
  "quickLook.label": (name) => `快速檢視：${name}`,
  "quickLook.open": "開啟服務頁面",
  "quickLook.upstream": "上游",
  "quickLook.downstream": "下游",

  "service.tabs": "服務頁面",
  "service.more": "更多",
  "service.views": "檢視方式",
  "service.delete": "刪除服務",
  "service.notFound": "找不到這個服務。",
  "service.created": (date) => `建立於 ${date}`,
  "tab.overview": "概覽",
  "tab.logs": "日誌",
  "tab.deployments": "部署紀錄",
  "tab.networking": "網路",
  "tab.variables": "環境變數",
  "tab.settings": "設定",
  "tab.terminal": "終端機",
  "tab.storage": "儲存空間",
  "tab.events": "事件",
  "tab.usage": "用量",
  "ops.restart": "重新啟動",
  "ops.later": "之後的版本才會提供。",
  "overview.started": (ago) => `${ago}啟動`,
  "overview.source": "來源",
  "overview.cluster": "叢集",
  "overview.managedBy": "管理工具",
  "overview.replicas": (ready, desired) => `副本 ${ready}/${desired}`,
  "overview.replicaSettings": "副本設定",
  "overview.noneReady": "沒有就緒的副本",
  "overview.someReady": (ready, desired) => `${desired} 個副本中 ${ready} 個就緒`,
  "overview.crashing": (n) => `${n} 個副本反覆崩潰`,
  "overview.manage": "管理",
  "overview.warnings": "警告",
  "pod.pods": "副本",
  "pod.none": "目前沒有執行中的副本。",
  "pod.ready": "就緒",
  "pod.notReady": "未就緒",
  "pod.readyCount": (ready, total) => `${ready}/${total} 就緒`,
  "pod.restarts": (n) => `重新啟動 ${n} 次`,
  "pod.logsOf": (pod) => `${pod} 的日誌`,
  "resources.title": "資源",
  "resources.hint": "Pod 範本裡的保留量與上限。要看實際用量需要 metrics-server。",
  "resources.container": "容器",
  "resources.cpu": "CPU",
  "resources.memory": "記憶體",
  "resources.requests": "保留量",
  "resources.limits": "上限",
  "connections.title": "連線",

  "logs.noPods": "沒有執行中的副本",
  "logs.noPodsBody": "日誌來自執行中的副本；副本啟動後，日誌就會出現在這裡。",
  "logs.podOf": (pod) => `副本 ${pod}`,
  "logs.replaced": (old, current) => `${old} 已被取代；改為顯示 ${current}。`,
  "logs.gone": (pod) => `${pod} 已不存在；正在找接替它的副本。`,
  "logs.container": "容器",
  "logs.previous": "前一次執行",
  "logs.timestamps": "時間戳記",
  "logs.wrap": "自動換行",
  "logs.follow": "持續追蹤",
  "logs.following": "正在追蹤",
  "logs.filter": "篩選",
  "logs.filterHint": "開頭加 - 可排除符合的行。",
  "logs.error": "錯誤",
  "logs.warn": "警告",
  "logs.info": "資訊",
  "logs.download": "下載",
  "logs.lines": "日誌內容",
  "logs.empty": "這個副本還沒有寫出任何日誌。",
  "logs.noMatch": "沒有符合的行。",
  "logs.jump": "跳到最新",
  "logs.ended": "串流已結束",
  "logs.connecting": "連線中…",
  "logs.resume": "繼續追蹤",
  "logs.refresh": "重新整理",
  "logs.unavailable.no-previous-run": "沒有前一次執行：容器還沒有重新啟動過。",
  "logs.unavailable.not-started": "還沒有日誌：容器尚未啟動。",
  "logs.unavailable.removed": "這次執行的容器和日誌都已從叢集移除。",
  "logs.unavailableFix.no-previous-run": "改看這次執行的日誌。",
  "logs.unavailableFix.not-started": "等容器啟動；副本的事件會說明它在等什麼。",
  "logs.unavailableFix.removed":
    "稍候再重新整理。要在重新啟動後保留日誌，可以把日誌送到日誌儲存服務。",
  "logs.currentRun": "看這次執行",
  "logs.count": (n) => `${n} 行`,
  "logs.of": (n) => `共 ${n} 行`,
  "terminal.label": "終端機",
  "terminal.noPodsBody": "終端機要在執行中的副本裡開啟；副本啟動後就能使用。",
  "terminal.pod": "副本",
  "terminal.connecting": "正在連線…",
  "terminal.open": "已連線",
  "terminal.reconnect": "重新連線",
  "terminal.exited": (code) => `已結束，結束碼 ${code}`,
  "terminal.ended": "已結束",
  "terminal.noShell":
    "這個容器沒有 shell，精簡的映像檔常是如此。除錯容器會在這個副本裡加上一個 shell，和應用程式並排執行。",
  "terminal.debugStarting": "正在啟動除錯容器…",
  "terminal.debugEnded": "除錯容器已結束。再開一個即可繼續。",
  "terminal.debugOpen": "已連線到除錯容器",
  "terminal.failed": "容器執行環境結束了這個工作階段。",
  "terminal.expired": "工作階段已過期。重新連線即可繼續。",
  "terminal.adminEnded": "管理員權限已結束。重新連線後會改用你自己的權限。",
  "terminal.idle": "15 分鐘沒有輸入或輸出，已關閉。",
  "terminal.lost": "連線中斷。",
  "terminal.rbac":
    "要開啟終端機，必須有權限在這個專案的容器裡執行指令。把下面的設定交給管理員；這份設定只授予這項權限，也只限這個專案：",
  "terminal.roleLabel": "Role 與 RoleBinding",
  "tools.about":
    "附有 kubectl 與 helm 的終端機，在這個專案裡以你的身分執行。它在一個暫時的容器裡執行，用完後 Zeabur 會刪除它。",
  "tools.localTitle": "Zeabur 在你的電腦上執行時無法使用",
  "tools.yours": "你的 kubectl 終端機",
  "tools.starting": "正在啟動…",
  "tools.ready": "可以使用",
  "tools.stopping": "正在停止",
  "tools.stopsAt": (when: string) => `${when}停止`,
  "tools.open": "開啟",
  "tools.stop": "停止",
  "tools.expired": "這個終端機裡的認證資訊已過期。重新開啟即可換上新的繼續使用。",
  "tools.reopen": "重新開啟",

  "vars.title": "環境變數",
  "vars.about": "應用程式啟動時讀取的設定，例如資料庫位址或 API 金鑰。",
  "vars.emptyTitle": "還沒有環境變數",
  "vars.emptyBody": "新增環境變數，不必建置新的映像檔就能改變設定。",
  "vars.direct": "直接設定",
  "vars.fromObjects": "來自設定檔和機密",
  "vars.fromCluster": "由 Kubernetes 填入",
  "vars.reveal": (secret) => `顯示 ${secret} 的值`,
  "vars.hide": (secret) => `隱藏 ${secret} 的值`,
  "vars.secretsHint": "關於 Secret 的值",
  "vars.revealHint": "Zeabur 只在你要顯示時才讀取值，而且只留在這個頁面。",
  "vars.view": "檢視方式",
  "vars.list": "清單",
  "vars.names": (service) => `指向服務 ${service}`,
  "vars.atRuntime": "執行時決定",
  "vars.allKeys": "所有鍵",
  "vars.keys": (n) => `${n} 個鍵`,

  "net.path": "路徑",
  "net.publicTitle": "公開網域",
  "net.publicAbout": "網際網路上的任何人都能開啟這些位址。",
  "net.publicEmpty": (name) => `${name} 還沒有公開網域`,
  "net.publicEmptyBody": "新增網域後，就能用瀏覽器開啟這個服務。",
  "net.publicNeedsPrivate": "網域會把請求轉送到服務的私有位址，所以要先有私有位址。",
  "net.sendsTo": "轉送到",
  "net.privateTitle": "私有網路",
  "net.privateAbout": "叢集裡的其他服務用這些位址連到它；網際網路連不到這些位址。",
  "net.privateEmpty": (name) => `${name} 沒有私有位址`,
  "net.privateEmptyBody": "有了私有位址，其他服務和它的公開網域才能把流量送給它。",
  "net.shortForm": "同專案內的簡寫",
  "net.forwardsTo": (port) => `轉送到容器的連接埠 ${port}`,
  "net.exposeHint": "要新增私有位址，請套用 Service 或執行這個指令。",
  "net.noneTitle": (name) => `${name} 無法接收連線`,
  "net.noneBody":
    "它沒有私有位址，也沒有公開網域。只會主動往外連的背景工作兩者都不需要，仍然可以連到其他服務和網際網路。",
  "net.scheduledTitle": (name) => `${name} 不接收流量`,
  "net.scheduledBody": "批次工作執行完就結束，不會有連線進來；它仍然可以連到其他服務和網際網路。",
  "reach.about": "叢集允許哪些連線進出這個服務。",
  "reach.fromDomains": "公開網域",
  "net.anyHost": "任何網域",
  "net.defaultBackend": "預設後端",

  "storage.requested": "申請容量",
  "storage.title": "磁碟區",
  "storage.about":
    "寫進磁碟區的檔案，在副本重新啟動或移到其他節點後仍會保留；容器裡其他地方的檔案，重新啟動後就會消失。",
  "storage.emptyTitle": (name) => `${name} 沒有磁碟區`,
  "storage.emptyBody":
    "應用程式要長期儲存檔案時（例如使用者上傳的檔案或資料庫），就新增一個磁碟區。",
  "storage.emptyJob": "要替 Job 新增磁碟區，請編輯它的 YAML。",
  "storage.bound": "就緒",
  "storage.pending": "等待儲存空間",
  "storage.lost": "儲存空間遺失",
  "storage.mountPath": "掛載路徑",
  "storage.accessMode": "存取模式",
  "storage.class": "儲存類別",
  "storage.defaultClass": "預設",
  "storage.otherMounts": "其他掛載",

  "palette.title": "指令面板",
  "palette.search": "前往專案、服務、副本或網域",
  "palette.recent": "最近",
  "palette.actions": "動作",
  "palette.openLogs": (name) => `開啟 ${name} 的日誌`,
  "palette.openTerminal": (name) => `開啟 ${name} 的終端機`,
  "palette.openVariables": (name) => `編輯 ${name} 的環境變數`,
  "palette.openChanges": (name) => `查閱 ${name} 的部署紀錄`,
  "palette.toggleTheme": "切換主題",
  "palette.components": "元件",
  "palette.empty": "找不到符合的項目。",
  "palette.in": (what, namespace) => `${namespace} 的 ${what}`,
  "palette.move": "移動",
  "palette.open": "開啟",

  "settings.title": "設定",
  "settings.preferences": "偏好設定",
  "settings.language": "語言",
  "settings.languageHint": "來自叢集的訊息維持英文。",
  "settings.theme": "主題",
  "settings.about": "關於",
  "settings.user": "登入身分",
  "settings.groups": "群組",
  "settings.kubernetesVersion": "Kubernetes",
  "settings.catalogueCommit": "Zeabur Catalog",
  "settings.components": "元件",
  "settings.filter": "篩選",
  "settings.searchComponents": "搜尋元件",
  "settings.installed": "已安裝",
  "settings.recommended": "推薦",
  "settings.optional": "選用",
  "settings.unsupported": "不支援",
  "settings.installCommand": "安裝",
  "settings.homepage": "首頁",
  "settings.opensInNewTab": "（在新分頁開啟）",
  "category.traffic": "流量",
  "category.observability": "可觀測性",
  "category.scaling": "擴縮",
  "category.security": "安全",
  "category.data": "資料",
  "category.operations": "維運",
  "category.other": "其他",
  "cap.certificates": "憑證",
  "cap.dns": "DNS 紀錄",
  "cap.event-autoscaling": "事件驅動自動擴縮",
  "cap.ingress": "Ingress",
  "cap.intrusion-prevention": "入侵防護",
  "cap.logs": "日誌儲存",
  "cap.metrics": "指標",
  "cap.node-updates": "節點更新",
  "cap.object-storage": "物件儲存",
  "cap.observability-operator": "監控 operator",
  "cap.postgres": "PostgreSQL 資料庫",
  "cap.resource-recommendations": "資源建議",
  "cap.runtime-security": "執行期安全",
  "cap.uptime-probes": "可用性監測",
  "cap.volume-backup": "磁碟區備份",
  "cap.vulnerability-scanning": "弱點掃描",
  // 物件與 YAML
  "project.entry.objects": "物件",
  // 專案的 Namespace
  "project.entry.terminal": "kubectl 終端機",
  "project.entry.settings": "設定",
  "action.adopt": "納入",
  "action.unadopt": "不再顯示為專案",
  "action.openDebugTerminal": "開啟除錯終端機",
  "action.openToolsTerminal": "開啟 kubectl 終端機",
  "action.saveDetails": "儲存詳細資料",
  "title.adopt": (ns) => `納入 ${ns}`,
  "title.unadopt": (ns) => `${ns} 不再顯示為專案`,
  "title.details": (ns) => `${ns} 的詳細資料`,
  "title.pauseAll": (ns) => `暫停 ${ns} 的所有服務`,
  "title.restartAll": (ns) => `重新啟動 ${ns} 的所有服務`,
  "confirm.adopt": "為這個 Namespace 加上專案標籤，裡面的物件都不會變。",
  "confirm.unadopt": "這個 Namespace 不再顯示為專案，裡面的物件都不會刪除。",
  "confirm.debugContainer": (pod: string) =>
    `在副本 ${pod} 加上一個附 shell 的除錯容器，它看得到應用程式的處理程序。`,
  "confirm.debugStays":
    "它會留在這個副本裡，直到副本被替換為止，例如重新啟動時。Kubernetes 無法提早移除它。",
  "confirm.debugReused": "這個副本裡已經有除錯容器在執行，終端機會在其中開啟。",
  "confirm.clusterTools": "為你在這個專案裡啟動一個暫時的容器，內含 kubectl 與 helm。",
  "confirm.toolsActsAs": (person: string, admin: boolean) =>
    admin
      ? `kubectl 與 helm 會以 ${person} 的身分執行，使用你目前持有的管理員權限。`
      : `kubectl 與 helm 會以 ${person} 的身分執行，使用你自己的權限。`,
  "confirm.toolsCredential": (minutes: number) =>
    `其中的認證資訊會在 ${minutes} 分鐘後失效，重新開啟終端機即可換上新的。`,
  "confirm.toolsStops": (most: number, after: number) =>
    `它最多執行 ${most} 分鐘，最後一個終端機關閉 ${after} 分鐘後就會刪除。`,
  "confirm.toolsReused": "你的 kubectl 終端機已在執行，會直接再次開啟。",
  "confirm.details": "把名稱、描述和圖示存到 Namespace 上。",
  "access.title": (ns) => `無法變更 ${ns}`,
  "access.you": "<你的使用者名稱>",
  "access.ask": (ns) => `向叢集管理員申請 Namespace ${ns} 的 patch 權限。`,
  "access.alsoLabels":
    "這個權限也能變更 Namespace 的其他標籤，包括 Pod 安全等級（pod-security.kubernetes.io/*）。Kubernetes 無法只限定一個標籤。",
  "access.role": "用 resourceNames 限定在這個 Namespace 的 ClusterRole：",
  "access.roleLabel": "ClusterRole 與綁定",
  "access.kubectl": "或由管理員執行：",
  "access.kubectlLabel": "kubectl 指令",
  "projectSettings.description": "描述",
  "projectSettings.icon": "圖示",
  "projectSettings.tooLong": (max) => `縮短到 ${max} 個字元以內。`,
  "projectSettings.readOnly": "可以檢視，但無法變更。",
  "projectSettings.getAccess": "如何取得權限",
  "projectSettings.changeDetails": "變更這個專案的資訊",
  "projectSettings.system": "這是 Kubernetes 留給自身元件的 Namespace，不能當作專案。",
  "projectSettings.title": "專案設定",
  "projectSettings.back": "返回",
  "projectSettings.general": "一般設定",
  "projectSettings.basic": "基本資訊",
  "projectSettings.iconHint": "顯示在專案名稱旁。",
  "projectSettings.changeIcon": "更換圖示",
  "projectSettings.name": "專案名稱",
  "projectSettings.nameHint": (ns) => `取代 ${ns} 顯示。Namespace 本身的名稱無法變更。`,
  "projectSettings.descriptionHint": "專案的用途，顯示在名稱下方。",
  "projectSettings.namespaceGroup": "Kubernetes Namespace",
  "projectSettings.namespace": "Namespace",
  "projectSettings.namespaceHint": "這個專案就是這個 Namespace，名稱無法變更。",
  "projectSettings.shown": "顯示為專案",
  "projectSettings.shownYes": "是。Namespace 帶有 Zeabur 的專案標籤。",
  "projectSettings.shownNo": "否。納入只會加上 Zeabur 的專案標籤，其他都不變。",
  "projectSettings.batch": "批次操作",
  "projectSettings.pauseAll": "暫停所有服務",
  "projectSettings.pauseAllHint": "停止這個專案的所有服務；之後可以恢復。",
  "projectSettings.pause": "暫停",
  "projectSettings.restartAll": "重新啟動所有服務",
  "projectSettings.restartAllHint": "分批替換副本，涵蓋所有能重新啟動的服務。",
  "projectSettings.danger": "危險區域",
  "projectSettings.remove": "從 Zeabur 移除",
  "projectSettings.removeHint": "只會移除 Zeabur 的專案標籤，Namespace 和其中的一切都會保留。",
  "projectSettings.removeAction": "移除",
  "icon.folder": "資料夾",
  "icon.globe": "地球",
  "icon.storefront": "商店",
  "icon.rocket": "火箭",
  "icon.cube": "方塊",
  "icon.database": "資料庫",
  "icon.cloud": "雲",
  "icon.code": "程式碼",
  "icon.terminal": "終端機",
  "icon.chart": "圖表",
  "icon.chat": "聊天",
  "icon.book": "書",
  "icon.flask": "燒瓶",
  "icon.game": "遊戲",
  "icon.heart": "愛心",
  "icon.briefcase": "公事包",
  "objects.filter": "篩選物件",
  "objects.generated": "自動產生",
  "objects.count": (n: number) => `${n} 個物件`,
  "objects.none": "這個專案沒有物件。",
  "objects.noMatch": (query) => `沒有符合「${query}」的物件。`,
  "objects.owner": (kind: string) => `由 ${kind} 建立`,
  "objects.empty": "空的",
  "objects.scan": "掃描全部種類",
  "objects.scanning": "正在掃描…",
  "objects.scanned": "已讀取全部種類",
  "objects.refresh": "重新整理",
  "objects.load": (kind: string) => `讀取 ${kind}`,
  "objects.unread": "未讀取",
  "objects.denied": "沒有權限",
  "objects.pin": (kind: string) => `釘選 ${kind}`,
  "objects.unpin": (kind: string) => `取消釘選 ${kind}`,
  "tab.yaml": "YAML",
  "yaml.hidden": "Secret 的值已隱藏。",
  "yaml.label": (name: string) => `${name} 的 YAML`,

  // 變更計畫
  "plan.planning": "正在計算…",
  "plan.apply": "套用",
  "plan.applying": "正在套用…",
  "plan.cancel": "取消",
  "plan.replan": "重新預覽",
  "plan.details": "技術細節",
  "plan.expiresIn": (time: string) => `${time} 後失效`,
  "plan.expired": "已失效",
  "plan.warnings": "套用前須知",
  "plan.owners": "欄位擁有者",
  "plan.when": (tool) =>
    tool === "Helm"
      ? "下次 Helm 升級時"
      : tool === "kubectl"
        ? "下次執行 kubectl apply 時"
        : `下次 ${tool} 同步時`,
  "plan.toolSkipped": (names, _count, tool, when) =>
    `略過 ${names}：由 ${tool} 管理，${when}會改回這個變更。`,
  "plan.toolPutBack": (when, paths, object) => `${when}會改回 ${object} 的 ${paths}。`,
  "plan.diff": "YAML 差異",
  "plan.create": "新增",
  "plan.delete": "刪除",
  "plan.typeName": (name: string) => `輸入 ${name} 以確認`,
  "plan.added": "新增",
  "plan.removed": "移除",
  "warn.rightsMissing": (rights: string) => `叢集可能會拒絕：你缺少 ${rights} 的權限。`,
  "warn.toolsCredentialExposed": (namespace: string) =>
    `在這個終端機的容器執行期間，能在 ${namespace} 的容器裡執行指令的人，都能使用你的認證資訊。`,
  "warn.restartNeeded": (names: string) => `重新啟動後才會生效：${names}`,
  "warn.restartAdded": (names: string) => `也會重新啟動 ${names}`,
  "warn.tlsHostChanged": (host: string, route: string) =>
    `${route} 的 HTTPS 設定仍是舊網域；${host} 需要新的憑證`,
  "warn.gatewayChosen": (gateway: string, of: number) =>
    `掛接到 Gateway ${gateway}（共找到 ${of} 個）`,
  "warn.gatewaysUnreadable": "無法讀取 Gateway：Zeabur 改用 Ingress",
  "warn.secretValuesUnknown": (name: string) => `無法讀取 ${name} 的值：差異只列出鍵名`,
  "warn.configNotRolledBack": (workload: string) =>
    `${workload} 讀取 ConfigMap 或 Secret：回滾不會改動它們的內容`,
  "warn.storageClassUnknown": (claim: string) =>
    `無法讀取 ${claim} 的 StorageClass：能否擴充由叢集決定`,
  "warn.readWriteOnceShared": (workload: string) =>
    `${workload} 有多個副本：ReadWriteOnce 磁碟區只能掛在一個節點上`,
  "warn.readWriteOncePodShared": (workload: string) =>
    `${workload} 有多個副本：ReadWriteOncePod 磁碟區一次只能給一個副本使用`,
  "warn.deletesDependents": (owner: string, names: string) =>
    `也會刪除 ${owner} 擁有的物件：${names}`,
  "warn.podReplaced": (pod: string, owner: string) => `${owner} 會啟動新的副本取代 ${pod}`,
  "warn.claimDataDeleted": (claim: string, volume: string) =>
    `叢集會刪除 ${claim} 的資料：磁碟區 ${volume} 的回收政策是 Delete`,
  "warn.claimDataRetained": (claim: string, volume: string) =>
    `磁碟區 ${volume} 會保留 ${claim} 的資料（回收政策 Retain），直到管理員移除`,
  "warn.claimDataUnknown": (claim: string) => `${claim} 的資料可能遺失：無法讀取其磁碟區的回收政策`,
  "warn.stillUsed": (name: string, names: string) =>
    `${names} 仍在讀取 ${name}：少了它，新的副本無法啟動`,
  "warn.claimStillMounted": (name: string, names: string) =>
    `${names} 仍掛載 ${name}：等到沒有副本使用後，Kubernetes 才會移除它`,
  "warn.ingressOpened": (names: string) => `不再隔離：${names} 會接受任何來源的流量`,
  "warn.egressOpened": (names: string) => `不再限制：${names} 可以把流量送到任何地方`,
  "warn.connectionsOpened": (pairs: string) => `恢復允許：${pairs}`,
  // 確認
  "confirm.restartSingle": (name) => `${name} 只有一個副本，重新啟動時可能中斷幾秒。`,
  "confirm.restartNone": (name) => `${name} 目前沒有就緒的副本，重新啟動會替換全部副本。`,
  "confirm.rolling": (name, pods) => `${name} 的 ${pods} 個副本會逐一替換，服務不會中斷。`,
  "confirm.rollingSingle": (name) => `${name} 只有一個副本，替換時可能中斷幾秒。`,
  "confirm.rollingNone": (name) => `${name} 的副本會全部替換。`,
  "confirm.envSet": (names, service) => `在 ${service} 設定 ${names}。`,
  "confirm.envRemove": (names, service) => `從 ${service} 移除 ${names}。`,
  "confirm.envBoth": (set, removed, service) => `在 ${service} 設定 ${set}，並移除 ${removed}。`,
  "confirm.dataSet": (keys, kind, name) => `在 ${kind} ${name} 設定 ${keys}。`,
  "confirm.dataRemove": (keys, kind, name) =>
    `從 ${kind} ${name} 刪除 ${keys}。刪除的值無法在這裡復原。`,
  "confirm.dataBoth": (set, removed, kind, name) =>
    `在 ${kind} ${name} 設定 ${set}，並刪除 ${removed}。刪除的值無法在這裡復原。`,
  "confirm.keepsOld": (names) => `${names} 在重新啟動前仍使用舊值。`,
  "confirm.readersKeepOld": (names, object) => `${names} 讀取 ${object}，在重新啟動前仍使用舊值。`,
  "confirm.alsoReads": (names, object) => `也會影響 ${names}（讀取 ${object}）。`,
  "confirm.alsoAffects": (names) => `也會影響 ${names}。`,
  "confirm.addDomain": (host, service) => `${host} 會連到 ${service}。`,
  "confirm.renameDomain": (from) => `連到 ${from} 的連結會失效。`,
  "confirm.removeDomain": (host) => `送到 ${host} 的請求不會再到達這個專案。`,
  "confirm.rollback": (revision) => `修訂版本 ${revision} 之後的範本變更都會撤銷。`,
  "confirm.notRolledBack": "回滾不會改動 ConfigMap 和 Secret 的內容。",
  "confirm.noShrink": "磁碟區擴充後就無法縮小。",
  "confirm.addVolume": (size, service) => `${service} 會多一個 ${size} 的磁碟區。`,
  "confirm.moveMount": (service, from, to) => `把 ${service} 的掛載從 ${from} 移到 ${to}。`,
  "confirm.readOnly": (path, service) => `把 ${service} 的 ${path} 設為唯讀。`,
  "confirm.writable": (path, service) => `把 ${service} 的 ${path} 設為可寫入。`,
  "confirm.mount": (service) => `修改 ${service} 的掛載。`,
  "confirm.probeSet.readiness": (check, period) =>
    `${check}成功之前，副本不會收到流量（每 ${period} 秒檢查一次）。`,
  "confirm.probeSet.liveness": (check, period, failures) =>
    `${check}連續失敗 ${failures} 次，叢集就會重新啟動容器（每 ${period} 秒檢查一次）。`,
  "confirm.probeSet.startup": (check, period, failures) =>
    `${check}成功之前，其他檢查先暫停；失敗 ${failures} 次後，叢集會重新啟動容器（每 ${period} 秒檢查一次）。`,
  "confirm.probeRemoved.readiness": "移除後，副本一啟動就會收到流量。",
  "confirm.probeRemoved.liveness": "移除後，叢集不會重新啟動卡住的容器。",
  "confirm.probeRemoved.startup": "移除後，其他檢查會在容器一啟動就開始。",
  "confirm.check.http": (port, path) => `對連接埠 ${port} 的 ${path} 發 GET 請求`,
  "confirm.check.tcp": (port) => `連線到連接埠 ${port}`,
  "confirm.check.grpc": (port) => `連接埠 ${port} 的 gRPC 健康檢查`,
  "confirm.check.exec": (command) => `執行 ${command} `,
  "confirm.scale": (name, from, to) => `把 ${name} 從 ${from} 個副本調整為 ${to} 個。`,
  "confirm.scaleTo": (name, to) => `把 ${name} 調整為 ${to} 個副本。`,
  "confirm.scaleZero": (name) => `${name} 會停止；再次調高副本數之前，不會有副本執行。`,
  "confirm.resources": (pairs) =>
    `${pairs.map(([label, value]) => `${label} ${value}`).join("，")}。`,
  "confirm.yamlAsIs": "Zeabur 會原樣寫入 YAML，你改動的每個欄位都會立即生效。",
  "confirm.create": (kind, name) => `建立 ${kind} ${name}。`,
  "confirm.isolates": (name) => `${name} 之後只收發這條和其他政策允許的流量，其餘流量叢集會擋下。`,
  "confirm.delete": (name: string) => `刪除 ${name}。刪除後無法復原。`,
  "action.restart": "重新啟動",
  "action.saveVariables": "儲存環境變數",
  "action.saveValues": "儲存值",
  "action.saveAndRestart": "儲存並重新啟動",
  "action.addDomain": "新增網域",
  "action.renameDomain": "更改網域",
  "action.removeDomain": "移除網域",
  "action.rollback": (revision) => `回滾到修訂版本 ${revision}`,
  "action.grow": (size) => `擴充到 ${size}`,
  "action.addVolume": "新增磁碟區",
  "action.saveMount": "儲存掛載",
  "action.saveHealthChecks": "儲存健康檢查",
  "action.scale": (replicas) => `調整為 ${replicas} 個副本`,
  "action.saveResources": "儲存資源",
  "action.save": "儲存",
  "action.saveYaml": "儲存 YAML",
  "action.isolate": "隔離",
  "action.allow": "允許",
  "action.create": "建立",
  "action.delete": "刪除",
  // 通知
  "toast.region": "通知",
  "toast.applied": "已套用",
  "toast.view": "檢視",
  "toast.undo": "復原",
  "toast.undone": "已復原",
  "toast.managed": (tool) => `由 ${tool} 管理`,
  "toast.managedHelm": "在 chart 的 values 裡修改，再升級 release。",
  "toast.managedGit": (tool) => `在 ${tool} 同步的 Git 儲存庫裡修改。`,
  "title.env": (service: string) => `${service} 的環境變數`,
  "title.data": (kind: string, name: string) => `${kind} ${name}`,
  "title.restart": (service: string) => `重新啟動 ${service}`,
  "title.addDomain": (host: string) => `新增 ${host}`,
  "title.renameDomain": (from: string, to: string) => `${from} → ${to}`,
  "title.removeDomain": (host: string) => `移除 ${host}`,
  "yaml.edit": "編輯",
  "yaml.editSecret": "維持 (hidden) 的值會保留原值。",
  "yaml.notEditable": "這類物件由 Kubernetes 自行管理，改為編輯擁有它的物件。",
  "title.editYaml": (name: string) => `編輯 ${name}`,
  "title.delete": (kind: string, name: string) => `刪除 ${kind} ${name}`,
  "yaml.delete": "刪除",
  "yaml.notDeletable": "這類物件由 Kubernetes 自行建立，改為刪除擁有它的物件。",
  "title.rollback": (service: string, revision: number) => `將 ${service} 回滾到 #${revision}`,
  "title.compute": (service: string) => `${service} 的運算資源`,
  "title.probe": (service: string) => `${service} 的健康檢查`,
  "title.expand": (claim: string, size: string) => `將 ${claim} 擴充到 ${size}`,
  "title.mount": (path: string) => `修改掛載 ${path}`,
  "title.editVolume": (claim) => `修改磁碟區 ${claim}`,
  "title.addVolume": (path: string) => `在 ${path} 新增磁碟區`,
  "releases.none": "每次更換映像檔或修改設定，這裡就會多一筆部署。",
  "releases.noneTask": "Job 和 CronJob 不保留發布紀錄。",
  "releases.current": "執行中",
  "releases.compare": "比較",
  "releases.rollback": "回滾",
  "releases.diff": (from: number, to: number) => `#${from} → #${to}`,
  "releases.same": "兩個範本相同。",
  "form.compute": "運算資源",
  "form.health": "健康檢查",
  "form.jobs": "Job 只執行一次，範本要用 YAML 修改。",
  "form.replicas": "副本數",
  "form.cpuRequest": "CPU 保留量",
  "form.notSet": "未設定",
  "form.cpuUnset": "未設定時不保留。250m 是四分之一核。",
  "form.memoryUnset": "未設定時不保留。例如 256Mi。",
  "form.noLimit": "未設定時不設上限",
  "form.memoryRequest": "記憶體保留量",
  "form.cpuLimit": "CPU 上限",
  "form.memoryLimit": "記憶體上限",
  "form.computeHint":
    "節點會依保留量替每個副本留出資源。副本用的記憶體超過上限時，容器會重新啟動；CPU 超過上限時會降速。",
  "form.largestNode": (cores: string, memory: string) => `最大的節點：${cores} 核、${memory}`,
  "form.badQuantity": "輸入數量，例如 250m、1 或 512Mi。",
  "form.badReplicas": "輸入 0 到 1000 的整數。",
  "form.badNumber": "輸入大於 0 的整數。",
  "form.badPath": "輸入絕對路徑，例如 /data。",
  "form.badName": "只能用小寫字母、數字和 -。",
  "form.probe.readiness": "就緒檢查",
  "form.probe.liveness": "存活檢查",
  "form.probe.startup": "啟動檢查",
  "form.probeEffect.readiness": "只有通過的副本會收到流量。",
  "form.probeEffect.liveness": "容器持續失敗時，叢集會重新啟動它。",
  "form.probeEffect.startup": "應用程式啟動前，先暫緩其他檢查。",
  "form.probeCheck": "檢查方式",
  "form.probeNone": "不檢查",
  "form.probeExec": "指令",
  "form.probePath": "路徑",
  "form.probePort": "連接埠",
  "form.probeCommand": "指令",
  "form.probeCommandMissing": "輸入要執行的指令。",
  "form.probePeriod": "間隔（秒）",
  "form.probeFailures": "失敗次數",
  "form.probePreset": (port: string) => `HTTP，連接埠 ${port}`,
  "form.size": "大小",
  "form.sizeHint": "只能擴充，不能縮小。",
  "form.readOnly": "唯讀",
  "form.readOnlyHint": "應用程式能讀取檔案，但不能修改。",
  "form.addVolume": "新增磁碟區",
  "form.volumeName": "名稱",
  "form.statefulVolumes": "StatefulSet 的磁碟區在建立時就固定了。",
  "form.advanced": "進階",
  "form.claimName": "PVC 名稱",
  "form.claimNameHint": "會建立新的 PVC；叢集會拒絕已在使用的名稱。",
  "form.badClaimName": "只能用小寫字母、數字、- 和 .，最多 253 個字元。",
  "form.accessMode": "存取模式",
  "form.accessModeHint": "多節點模式需要支援它的儲存空間。",
  "ops.managedEdit": (tool: string) => `由 ${tool} 管理，要在它的來源修改`,

  // 編輯
  "edit.add": "新增",
  "edit.review": "預覽變更",
  "edit.cancel": "取消",
  "edit.edit": (name: string) => `編輯 ${name}`,
  "edit.remove": (name: string) => `移除 ${name}`,
  "edit.undo": (name: string) => `保留 ${name}`,
  "vars.edit": "編輯",
  "vars.addRow": "新增變數",
  "vars.pasteHint": "在名稱欄貼上多行 NAME=value，可以一次全部加入。",
  "vars.dotenv": ".env 格式的環境變數",
  "vars.dotenvHint": "一行一個 NAME=value。可以用引號、export 和 # 註解；拿掉的那一行會被移除。",
  "vars.unread": (lines: number[]) => `第 ${lines.join("、")} 行不是 NAME=value 格式。`,
  "vars.blocked": (n: number) => `修正 ${n} 個名稱後才能儲存。`,
  "vars.edited": (n: number) => `${n} 項變更`,
  "vars.state.added": "新增",
  "vars.state.changed": "已修改",
  "vars.state.removed": "將移除",
  "vars.undo": "復原",
  "vars.namePlaceholder": "NAME",
  "vars.valueOf": (name: string) => `${name} 的值`,
  "vars.problem.empty": "請輸入名稱。",
  "vars.problem.invalid": "只能用英文字母、數字、_、- 和 .，而且不能以數字開頭。",
  "vars.problem.duplicate": "另一個變數已經用了這個名稱。",
  "vars.problem.taken": "設定檔、機密或 Kubernetes 已經設定了這個名稱。",
  "vars.editThere": "到存放它的地方修改",
  "vars.imported": "設定檔或機密的所有鍵",
  "vars.inSecret": (secret: string, key: string) => `機密 ${secret} 的 ${key}`,
  "vars.allOfSecret": (secret: string) => `機密 ${secret} 的所有鍵`,
  "vars.inConfig": (file: string, key: string) => `設定檔 ${file} 的 ${key}`,
  "vars.allOfConfig": (file: string) => `設定檔 ${file} 的所有鍵`,
  "vars.fromFile": (path: string) => `檔案 ${path}`,
  "vars.field.podName": "副本名稱",
  "vars.field.namespace": "專案的 Namespace",
  "vars.field.podIP": "副本的 IP 位址",
  "vars.field.hostIP": "節點的 IP 位址",
  "vars.field.nodeName": "節點名稱",
  "vars.field.serviceAccount": "副本的服務帳戶",
  "vars.field.cpuLimit": "CPU 上限",
  "vars.field.memoryLimit": "記憶體上限",
  "vars.field.cpuRequest": "CPU 保留量",
  "vars.field.memoryRequest": "記憶體保留量",
  "vars.field.other": (path: string) => `副本的 ${path}`,
  "kube.details": "給 Kubernetes 使用者的細節",
  "vars.add": "新增環境變數",
  "vars.name": "名稱",
  "vars.value": "值",
  "vars.container": "容器",
  "data.addKey": "新增鍵",
  "data.key": "鍵",
  "data.restartAfter": "套用後重新啟動讀取它的服務",
  "data.readers": "讀取者",
  "data.reveal": "顯示值",
  "data.binary": "二進位",
  "data.noChanges": "沒有變更。",
  "net.addDomain": "新增網域",
  "net.domain": "網域",
  "diagnosis.title": "出了什麼問題",
  "diagnosis.evidence": "依據",
  "diagnosis.replica": "副本",
  "diagnosis.exitCode": "結束碼",
  "diagnosis.action.logs": "開啟日誌",
  "diagnosis.action.memory": "調整記憶體",
  "diagnosis.action.healthChecks": "健康檢查",
  "diagnosis.action.networking": "網路",
  "diagnosis.action.events": "開啟事件",
  "diagnosis.action.replicas": "調整副本數",
  "diagnosis.action.reservation": "調整保留量",
  "diagnosis.action.variables": "環境變數",
  "diagnosis.action.storage": "儲存空間",
  "gap.title": "缺少的設定",
  "gap.warning": "警告",
  "gap.advice": "建議",
  "gap.unchecked": (count) => `${count} 個服務沒有就緒檢查`,
  "gap.uncheckedWhy": "沒有就緒檢查時，「執行中」只代表容器已啟動。每個服務都在「設定」中設定。",
  "gap.uncheckedNames": "顯示服務",
  "gap.fix": "修正",
  "gap.install": "安裝",
  "gap.snippet": "要加入的 YAML",
  "fix.resources": (container: string) => `為 ${container} 設定 CPU 和記憶體`,
  "fix.readiness": (container: string) => `為 ${container} 加上就緒檢查`,
  "fix.addVolume": (container, path) => `把 ${container} 的 ${path} 存到磁碟`,
  "fix.isolateProject": (project: string) => `隔離 ${project} 的服務`,
  "gap.unreadable": "部分檢查需要你沒有的權限。",
  "install.title": "安裝",
  "install.note": "Zeabur 不會執行這些內容。自己執行，或提交到 GitOps 儲存庫。",
  "install.profile": "安裝方式",
  "install.singleNode": "單一節點",
  "install.multiNode": "多個節點",
  "install.forCluster": "適合這個叢集",
  "install.needs": "需要先安裝",
  "install.security": "權限",
  "install.clusterWide": "叢集範圍的 RBAC",
  "install.privileged": "特權模式",
  "install.connectsTo": "會連到",
  "install.credentials": "認證資訊",
  "install.risks": "風險",
  "install.inputs": "需要填入的值",
  "install.generated": "自動產生",
  "install.required": "必填",
  "install.alsoAdds": "也會為這些元件加入值",
  "install.k3sPackaged": (addon: string) =>
    `在 k3s 上，先停用內建的 ${addon}：在每台 server 的 /etc/rancher/k3s/config.yaml 把它加進 disable，再重新啟動 k3s。`,
  "install.k3sPackagedTraefik":
    "這會刪除 Traefik 的 CRD 與所有 IngressRoute、Middleware 物件；這個元件開始執行前，Ingress 流量會中斷。",
  "install.k3sPackagedMetrics": "這個元件開始執行前，metrics API 無法使用。",
  "install.k3sPackagedOther": "這個元件開始執行前，內建版本的工作負載會停止。",
  "reach.title": "連線規則",
  "reach.addPolicy": "新增規則",
  "reach.isolate": "隔離這個服務",
  "reach.allowIngress": "允許來自公開網域的流量",
  "reach.allowFrom": (name) => `允許 ${name} 連入`,
  "reach.planIsolate": (name) => `隔離 ${name}`,
  "reach.planAllowIngress": (name) => `允許公開網域連到 ${name}`,
  "reach.planAllowFrom": (from, to) => `允許 ${from} 連到 ${to}`,
  "reach.policies": "規則",
  "reach.noPolicies": "這個專案沒有連線規則，所以允許所有連線。",
  "reach.noneSelect": "沒有規則套用到這個服務，所以允許所有連線。",
  "reach.in": "連入",
  "reach.out": "連出",
  "reach.otherNamespaces": "其他 Namespace",
  "reach.dns": "名稱查詢",
  "reach.outside": "叢集外",
  "reach.cni": "規則是否真的生效，取決於叢集的網路外掛（CNI）。",
  "reach.allowed": "允許",
  "reach.limited": "部分",
  "reach.blocked": "封鎖",
  "reach.edgeBlocked": "連線規則不允許",
  "reach.edgeLimited": "連線規則只允許一部分",
  "backups.title": "備份",
  "backups.namespace": "Namespace 內的所有物件",
  "backups.unscheduled": "沒有排程",
  "backups.none": "沒有備份",
  "backups.noRun": "還沒備份過",
  "backups.noProvider": "要備份磁碟區，請先安裝備份元件。",
  "backups.seeComponents": "開啟元件",
  "backups.refused": (module) => `沒有權限讀取 ${module} 的備份。`,
  "backups.details": "排程與紀錄",
  "backups.paused": "已暫停",
  "backups.succeeded": "成功",
  "backups.partiallyFailed": "部分失敗",
  "backups.failed": "失敗",
  "backups.running": "執行中",
  "backups.unknown": "不明",
  "backups.veleroVolumesNeedSnapshots": "磁碟區資料需搭配快照",
  "backups.scheduledOf": (scheduled, total) => `${scheduled}/${total} 有排程`,

  // 用量與歷史
  "usage.title": "目前用量",
  "usage.history": "歷史紀錄",
  "usage.live": "即時",
  "usage.liveHint":
    "數值為近似值，來自 metrics-server，大約每 15 秒量一次。記憶體是 working set，也就是 kubelet 據以判斷的數字。刻度是每個副本的保留量（request）；長條的盡頭是上限（limit），沒有上限時畫成斜線。",
  "usage.cores": (n) => `${n} 核`,
  "usage.coresUnit": "核",
  "usage.usedWord": (v) => `已用 ${v}`,
  "usage.reservedWord": (v) => `保留 ${v}`,
  "usage.maxWord": (v) => `上限 ${v}`,
  "usage.noMax": "沒有上限",
  "usage.noReserved": "沒有保留量",
  "usage.acrossPods": (n) => `（${n} 個副本合計）`,
  "usage.ofMax": (pct) => `上限的 ${pct}`,
  "usage.sampledOf": (a, b) => `已量到 ${a}/${b} 個副本`,
  "usage.perPodTemplate": (n, cpu, memory) =>
    `每個副本（×${n}）：CPU ${cpu}，記憶體 ${memory}（保留量 / 上限）`,
  "usage.replica": "副本",
  "usage.notes": "備註",
  "usage.nearLimitOne": (pod) => `${pod} 快用到記憶體上限，超過就會重新啟動。`,
  "usage.nearLimitMany": (n) => `${n} 個副本快用到記憶體上限，超過就會重新啟動。`,
  "usage.needsMetricsServer": "即時用量需要 metrics-server。",
  "usage.install": "安裝",
  "usage.whatLive": "檢視即時用量",
  "usage.unavailable": "metrics-server 沒有回應，因此只列出保留量與上限。",
  "usage.note.oom": "記憶體不足",
  "usage.note.oomHint": "上次以 OOMKilled 結束：用量超過記憶體 limit，容器因此重新啟動。",
  "usage.note.nearLimit": "接近記憶體上限",
  "usage.note.nearLimitHint": (pct) =>
    `已用記憶體 limit 的 ${pct}。超過 limit 時，容器會停止並重新啟動（OOMKilled）。`,
  "usage.note.cpuAtLimit": "CPU 已達上限",
  "usage.note.cpuAtLimitHint": "受 CPU limit 限制（throttling）：會變慢，但不會重新啟動。",
  "usage.note.overRequest": "超過保留量",
  "usage.note.overRequestHint":
    "用的記憶體比 request 多。機器記憶體不足時，kubelet 會先驅逐超過 request 的副本。",
  "usage.note.noSample": "還沒有資料",
  "usage.note.noSampleHint": "metrics-server 只量測執行中的容器；新副本大約 30 秒內出現。",
  "usage.split": "顯示",
  "usage.total": "合計",
  "usage.perReplica": "每個副本",
  "usage.range": "範圍",
  "usage.range.1h": "1 小時",
  "usage.range.6h": "6 小時",
  "usage.range.24h": "24 小時",
  "usage.range.7d": "7 天",
  "usage.rangeLong.1h": "1 小時",
  "usage.rangeLong.6h": "6 小時",
  "usage.rangeLong.24h": "24 小時",
  "usage.rangeLong.7d": "7 天",
  "usage.reservedLine": "保留量",
  "usage.maxLine": "上限",
  "usage.chartLabel": (metric, range) => `${metric}，最近 ${range}`,
  "usage.noSampleShort": "沒有資料",
  "usage.replacedPods": (n) => `${n} 個已替換的副本（淡色）`,
  "usage.markerOom": "記憶體不足",
  "usage.markerRestart": "重新啟動",
  "usage.restartsUnknown": "重新啟動次數不明",
  "usage.restartsUnknownHint": "Metrics 儲存服務沒有重新啟動次數：它沒有收集 kube-state-metrics。",
  "usage.needsStore": "歷史紀錄需要 metrics 儲存服務。",
  "usage.installVm": "安裝 VictoriaMetrics",
  "usage.chooseStore": "改用其他儲存服務",
  "usage.needsStoreHint": (service) =>
    `Zeabur 找的是 ${service}。相容 Prometheus 的儲存服務都可以用，例如 Prometheus 或 VictoriaMetrics；在設定裡指定它的 Service。`,
  "usage.whatHistory": "檢視用量歷史紀錄",
  "usage.proxyHint":
    "Zeabur 以你的身分透過 Kubernetes API 讀取 metrics 儲存服務。能這樣讀取的人可以看到所有專案的 metrics，所以通常只有管理員有這項權限。",
  "usage.storeUnavailable": "Metrics 儲存服務沒有回應。",
  "usage.noSeries": (name, range) => `最近 ${range} 沒有 ${name} 的資料。`,
  "usage.noSeriesHint":
    "儲存服務有回應，但沒有這些副本的資料：可能沒有收集這台機器的容器 metrics，或沒有保留這麼久。",
  "usage.source": "來源",
  "usage.stepSeconds": (n) => `每 ${n} 秒一點`,
  "usage.stepMinutes": (n) => `每 ${n} 分鐘一點`,
  "usage.tenancy": "誰看得到",
  "usage.tenancyHint":
    "Zeabur 只查詢這個專案的副本。但儲存服務本身不分專案：能開啟它的人可以讀到所有專案的 metrics。",
  "usage.loadingChart": "正在載入圖表…",

  // 設定：metrics 儲存服務
  "settings.metricsStore": "用量歷史",
  "settings.store": "歷史資料來源",
  "settings.storeHint":
    "保存用量歷史的 Service，格式為 namespace/name:port。任何提供 Prometheus HTTP API 的儲存服務都可以。Zeabur 透過 Kubernetes API，以每個人自己的權限讀取。",
  "settings.storeDefault": "預設",
  "settings.storeChosen": "在 Zeabur 中設定",
  "settings.storeState": "狀態",
  "settings.storeState.available": "有回應",
  "settings.storeState.not-configured": "找不到",
  "settings.storeState.unreadable": "沒有權限",
  "settings.storeState.unavailable": "沒有回應",
  "settings.storeSave": "儲存",
  "settings.storeReset": "改回預設",
  "settings.storeSaved": "已儲存",
  "settings.storeWho": "通常只有叢集管理員能修改。",
  "settings.storeWhereLabel": "設定存放位置",
  "settings.storeWhere": (namespace) =>
    `存放在 Namespace ${namespace} 的 ConfigMap zeabur-settings。能修改這個 ConfigMap 的人，就能修改這項設定。`,
  "settings.storeDenied": "你不能修改這項設定。",
  "settings.storeChange": "變更歷史資料來源",
  "settings.storeLocal":
    "Zeabur 在這台電腦上執行，不保存設定。要改用其他儲存服務，以 --metrics-service namespace/name:port 啟動。",
  "settings.storeTenancy":
    "能讀取儲存服務的人，就能看到所有專案的指標。只開放給可以檢視整個叢集的人。",
  "sentence.problem.forbidden.fix.askForRule": (p) =>
    `向叢集管理員要求 ${p.rule} 的權限，例如建立授予這項權限的 ClusterRole，以及綁定到你的 ClusterRoleBinding。`,
  "sentence.problem.forbidden.fix.askOrRetry": () => "洽詢管理員，或以管理員身分重試。",
  "sentence.problem.forbidden.fix.sendGrant": () =>
    "把下面的設定交給管理員。它只授予這項權限，而且只在這個專案有效。",
  "sentence.problem.planExpired.fix.again": (p) =>
    `計畫只在 ${p.minutes} 分鐘內有效。重新產生計畫。`,
  "sentence.problem.badRequest.fix.planToken": () =>
    "重新產生計畫。計畫只能在產生它的伺服器上，用原本的權杖原封不動地套用。",
  "sentence.problem.conflict.fix.review": () => "重新預覽，從它目前的狀態開始。",
  "sentence.problem.conflict.fix.waitForContainer": () =>
    "等它開始執行；它的日誌和事件會說明為什麼還沒執行。",
  "sentence.problem.unavailable.fix.tryAgainSoon": () => "稍後再試一次。",
  "sentence.problem.logUnavailable.fix.noPreviousRun": () => "改看這一次執行的日誌。",
  "sentence.problem.logUnavailable.fix.notStarted": () =>
    "等容器啟動；副本的事件會說明它在等什麼。",
  "sentence.problem.logUnavailable.fix.removed": () =>
    "稍候再重新整理。如果還是無法讀取，改看這一次執行的日誌。要在重新啟動後保留日誌，把日誌送到日誌儲存服務。",
  "sentence.problem.quotaExceeded.fix.scale": (p) =>
    p.object
      ? `減少 ${p.object} 的副本數、調低每個副本的保留量，或請管理員調高上限 ${p.quota}。`
      : `減少要求的量、釋出專案已用的資源，或請管理員調高上限 ${p.quota}。`,
  "sentence.problem.policyDenied.fix.change": () => "修改它指出的地方，或洽詢負責這項政策的人。",
  "sentence.problem.invalid.fix.ephemeralContainers": () =>
    "請叢集管理員開啟臨時容器（Kubernetes 1.23 起預設開啟），或改用附 shell 的映像檔。",
  "sentence.problem.conflict.fix.terminalImage": (p) =>
    `請叢集管理員讓節點能拉取 ${p.image}，或在 Zeabur 的 chart 裡把 ${p.setting} 設成節點拉得到的映像檔。`,
  "sentence.problem.conflict.fix.terminalContainerStart": () =>
    "從副本的事件找出原因；可能是某項政策不允許它所用的使用者身分。",
  "sentence.problem.badRequest.fix.toolsLocally": () =>
    "在你自己的終端機執行 kubectl 與 helm；它們和 Zeabur 用的是同一份 kubeconfig。",
  "sentence.diagnosis.crashLoop.why.backOff": (p) =>
    `它已重新啟動 ${p.restarts} 次；叢集每次都會再啟動它，兩次之間等得越來越久，預設最多等 5 分鐘。`,
  "sentence.diagnosis.crashLoop.why.exitsZero": (p) =>
    `服務應該持續執行，所以程式一結束，即使是正常結束，叢集也會再啟動它（目前 ${p.restarts} 次）。`,
  "sentence.diagnosis.runFailed.why.exitCode": (p) =>
    `程式結束時的結束碼不是 0 就算執行失敗；這次是 ${p.exitCode}。`,
  "sentence.diagnosis.runFailed.why.gaveUp": (p) =>
    p.backoffLimit
      ? p.backoffLimit === "0"
        ? `這項工作失敗後不會重試；這次的結束碼是 ${p.exitCode}。`
        : `這項工作已放棄：執行失敗時會重試 ${p.backoffLimit} 次，最後一次的結束碼也是 ${p.exitCode}。`
      : `這項工作重試後已放棄；最後一次的結束碼是 ${p.exitCode}。`,
  "sentence.diagnosis.outOfMemory.why.limit": (p) =>
    `每個副本最多只能用 ${p.limit} 記憶體，用到上限時系統會直接終止它，程式來不及處理。`,
  "sentence.diagnosis.outOfMemory.why.noLimit": () =>
    "沒有記憶體上限時，它可以一直用到機器的記憶體不足，這時系統會終止用最多的程式。",
  "sentence.diagnosis.imagePull.why": () =>
    "叢集要先下載映像檔才能啟動容器；下載失敗時會一再重試，每次等得更久。",
  "sentence.diagnosis.probeFailure.why.liveness": (p) => {
    const t = probeTiming(p)
    return t
      ? timingZhTW(
          t,
          `叢集每 ${secondsZh(t.period)}檢查一次，連續失敗 ${t.threshold} 次（約 ${secondsZh(t.total)}）就會重新啟動容器。`,
        )
      : "存活檢查連續失敗幾次後，叢集就會重新啟動容器。"
  },
  "sentence.diagnosis.probeFailure.why.readiness": (p) => {
    const t = probeTiming(p)
    return t
      ? timingZhTW(
          t,
          `叢集每 ${secondsZh(t.period)}檢查一次；連續失敗 ${t.threshold} 次（約 ${secondsZh(t.total)}）後，容器就不會從私有位址收到流量，直到再次通過。`,
        )
      : "就緒檢查失敗期間，容器不會從私有位址收到流量。"
  },
  "sentence.diagnosis.probeFailure.why.startup": (p) => {
    const t = probeTiming(p)
    return t
      ? timingZhTW(
          t,
          `容器有 ${t.threshold} × ${secondsZh(t.period)}（${secondsZh(t.total)}）可以通過啟動檢查；超過時叢集就會重新啟動它。`,
        )
      : "其他檢查會等啟動檢查通過；如果沒有在時限內通過，叢集就會重新啟動容器。"
  },
  "sentence.diagnosis.configMissing.cause.object": (p) =>
    `${p.container} 無法啟動：${objectZhTW(p)} 不存在。`,
  "sentence.diagnosis.configMissing.cause.key": (p) =>
    `${p.container} 無法啟動：${objectZhTW(p)} 裡沒有 ${p.key}。`,
  "sentence.diagnosis.configMissing.cause.unknown": (p) =>
    `${p.container} 無法啟動：缺少它讀取的機密或設定檔。`,
  "sentence.diagnosis.configMissing.cause.volume": (p) =>
    `${mountedZhTW(p)} 無法啟動：磁碟區 ${p.volume} 取自 ${objectZhTW(p)}，但它不存在。`,
  "sentence.diagnosis.configMissing.why": () =>
    "容器讀取的每個機密、設定檔和其中的鍵都存在之後，叢集才會啟動它。",
  "sentence.diagnosis.configMissing.why.volume": () =>
    "叢集會先掛載所有磁碟區，再啟動副本的容器；磁碟區如果取自不存在的機密或設定檔，就無法掛載，容器會一直等待。",
  "sentence.diagnosis.configMissing.fix.object": (p) =>
    `在專案裡加上 ${objectZhTW(p)}，或不再讀取它。`,
  "sentence.diagnosis.configMissing.fix.key": (p) =>
    `在 ${objectZhTW(p)} 加上 ${p.key}，或不再讀取它。`,
  "sentence.diagnosis.configMissing.fix.unknown": () =>
    "副本的事件會說缺少什麼；把它加上，或不再讀取它。",
  "sentence.diagnosis.configMissing.fix.volume": (p) =>
    `在專案裡加上 ${objectZhTW(p)}，或移除磁碟區 ${p.volume}。`,
  "sentence.diagnosis.unschedulable.cause.cpu": (p) =>
    `${p.service} 正在等待可用資源：沒有機器有足夠的 CPU。`,
  "sentence.diagnosis.unschedulable.cause.memory": (p) =>
    `${p.service} 正在等待可用資源：沒有機器有足夠的記憶體。`,
  "sentence.diagnosis.unschedulable.cause.placement": (p) =>
    `${p.service} 正在等待可以執行它的機器。`,
  "sentence.diagnosis.unschedulable.cause.other": (p) => `${p.service} 正在等待可以執行的機器。`,
  "sentence.diagnosis.unschedulable.why": (p) =>
    `每個副本要放到容得下它保留量的機器上；叢集的說明：${p.message}`,
  "sentence.diagnosis.unschedulable.fix.cpu": () => "減少保留的 CPU、減少副本數，或加一台機器。",
  "sentence.diagnosis.unschedulable.fix.memory": () =>
    "減少保留的記憶體、減少副本數，或加一台機器。",
  "sentence.diagnosis.unschedulable.fix.placement": () =>
    "檢查它可以在哪裡執行（node selector、affinity、tolerations）。",
  "sentence.diagnosis.unschedulable.fix.other": () => "副本事件裡有排程器的完整回應。",
  "sentence.diagnosis.quotaBlocked.cause": (p) =>
    `${p.service} 無法再啟動更多副本：已達到專案用量上限 ${p.quota}。`,
  "sentence.diagnosis.quotaBlocked.why": (p) =>
    `專案用量上限 ${p.quota} 限制所有服務加起來能保留的量；超過時叢集會拒絕新的副本。`,
  "sentence.diagnosis.quotaBlocked.fix": () =>
    "減少副本數、調低每個副本的保留量，或請管理員調高上限。",
  "sentence.diagnosis.unreachable.cause.noService": (p) =>
    `叢集裡的其他服務還連不到 ${p.service}：它沒有私有位址（Service）。`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p) =>
    `${p.endpoint} 把流量送到連接埠 ${p.targetPort}，${p.service} 的容器卻沒有宣告它${p.ports ? `；容器宣告的是 ${p.ports}` : ""}。`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p) =>
    `${p.endpoint ?? p.address} 沒有地方可以送流量：${p.service} 沒有任何就緒的副本。`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p) =>
    `送到 ${p.route} 網域的請求到不了 ${p.service}：流量入口回報 ${p.condition} ${p.reason}。`,
  "sentence.diagnosis.unreachable.why.noService": (p) =>
    p.from
      ? `${p.from} 用名稱 ${p.service} 送流量給它，但要有同名的私有位址（Service）才收得到。`
      : `路由用名稱 ${p.endpoint ?? p.service} 送流量，但要有同名的私有位址（Service）才收得到。`,
  "sentence.diagnosis.unreachable.why.targetPort": () =>
    "私有位址會轉送到它指定的連接埠；用名稱指定時，必須是容器宣告的連接埠。",
  "sentence.diagnosis.unreachable.why.noReadyEndpoints": () =>
    "私有位址只會把流量送給通過就緒檢查的副本。",
  "sentence.diagnosis.unreachable.why.routeNotAccepted": () =>
    "網域的流量入口必須接受這條路由，並找到它指定的位址，請求才會通過。",
  "sentence.diagnosis.unreachable.fix.noService": () =>
    "新增私有位址（Service），讓其他服務或網域可以把流量送給它。",
  "sentence.diagnosis.unreachable.fix.targetPort": () => "讓私有位址指向容器宣告的連接埠。",
  "sentence.diagnosis.unreachable.fix.noReadyEndpoints": () =>
    "找出為什麼沒有副本就緒：它的其他問題、就緒檢查、日誌。",
  "sentence.diagnosis.unreachable.fix.routeNotAccepted": () =>
    "檢查路由用的流量入口，以及路由指向的位址；路由的狀態會說是哪一個。",
  "sentence.diagnosis.unexplained.cause.waiting": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 正在等待（${p.reason}）。`
      : `${p.pod} 沒有在執行：${p.container} 正在等待（${p.reason}）。`,
  "sentence.diagnosis.unexplained.cause.terminated": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 已停止（${p.reason}）。`
      : `${p.pod} 沒有在執行：${p.container} 已停止（${p.reason}）。`,
  "sentence.diagnosis.unexplained.cause.pending": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 還沒啟動（Pending）。`
      : `${p.pod} 還沒啟動（Pending）。`,
  "sentence.diagnosis.unexplained.cause.notReady": (p) => {
    const state = p.check ? `在執行，但沒有通過就緒檢查（${p.check}）` : "在執行，但還沒就緒"
    return p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} ${state}。`
      : `${p.pod} ${state}。`
  },
  "sentence.diagnosis.unexplained.cause.phase": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 的狀態是 ${p.phase}。`
      : `${p.pod} 的狀態是 ${p.phase}。`,
  "sentence.diagnosis.unexplained.cause.none": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了。`
      : p.reason
        ? `${p.service} 異常（${p.reason}）。`
        : `${p.service} 異常。`,
  "sentence.diagnosis.unexplained.why.event": (p) =>
    `關於它的最新事件：${p.event}，「${p.note}」。${servingZhTW(p)}`,
  "sentence.diagnosis.unexplained.why.noEvent": (p) => `叢集沒有關於它的事件。${servingZhTW(p)}`,
  "sentence.diagnosis.unexplained.fix.waiting": (p) =>
    `開啟 ${p.pod} 的事件，kubelet 會在那裡說明它在等什麼。`,
  "sentence.diagnosis.unexplained.fix.terminated": (p) =>
    `開啟 ${p.pod} 的日誌和事件，看它為什麼停止。`,
  "sentence.diagnosis.unexplained.fix.pending": (p) =>
    `開啟 ${p.pod} 的事件，排程器和 kubelet 會在那裡說明它為什麼還沒啟動。`,
  "sentence.diagnosis.unexplained.fix.notReady": (p) =>
    `確認應用程式回應就緒檢查探測的位置，再看 ${p.pod} 的日誌。`,
  "sentence.diagnosis.unexplained.fix.phase": (p) => `開啟 ${p.pod} 的事件和日誌。`,
  "sentence.diagnosis.unexplained.fix.none": (p) => `開啟 ${p.service} 的事件。`,
  "sentence.problem.summary.bad-request": () => "這個請求無效。",
  "sentence.problem.summary.unauthenticated": () => "叢集不接受你的登入。",
  "sentence.problem.summary.signed-out": () => "你已登出。",
  "sentence.problem.summary.forbidden": (p) =>
    p.verb ? `你不能${verbZhTW[p.verb] ?? p.verb} ${thingZh(p, "裡的")}。` : "你沒有權限。",
  "sentence.problem.summary.elevation-required": (p) =>
    p.verb
      ? `你平常的權限不能${verbZhTW[p.verb] ?? p.verb} ${thingZh(p, "裡的")}。`
      : "這需要管理員權限。",
  "sentence.problem.summary.not-found": (p) => (p.name ? `${p.name} 不存在。` : "它不存在。"),
  "sentence.problem.summary.log-unavailable": () => "無法讀取這份日誌。",
  "sentence.problem.summary.conflict": (p) =>
    p.name ? `你預覽之後，${p.name} 又有了變更。` : "你預覽之後，它又有了變更。",
  "sentence.problem.summary.managed": (p) =>
    p.name ? `${p.name} 由其他工具管理。` : "這由其他工具管理。",
  "sentence.problem.summary.plan-expired": () => "計畫已過期。",
  "sentence.problem.summary.system-namespace": (p) =>
    p.name ? `${p.name} 是 Kubernetes 自用的 Namespace。` : "這是 Kubernetes 自用的 Namespace。",
  "sentence.problem.summary.invalid": (p) =>
    p.name ? `叢集拒絕了對 ${p.name} 的變更。` : "叢集拒絕了這項變更。",
  "sentence.problem.summary.too-many-requests": () => "叢集正忙。",
  "sentence.problem.summary.unavailable": () => "連不到叢集。",
  "sentence.problem.summary.internal": () => "Zeabur 內部發生錯誤。",
  "sentence.problem.summary.quota-exceeded": (p) =>
    p.name ? `變更 ${p.name} 會超過專案的用量上限。` : "這會超過專案的用量上限。",
  "sentence.problem.summary.policy-denied": (p) =>
    p.name ? `叢集裡的政策不允許變更 ${p.name}。` : "叢集裡的政策不允許這項變更。",
  "sentence.problem.why.quota-exceeded": (p) =>
    p.resource
      ? `專案用量上限 ${p.quota} 允許 ${p.resource} 最多 ${p.hard}，目前已用 ${p.used}。`
      : `專案用量上限 ${p.quota} 已經沒有空間。`,
  "sentence.problem.why.policy-denied": (p) =>
    p.policy ? `政策 ${p.policy} 的回應是：` : "叢集的回應是：",
  "sentence.diagnosis.crashLoop.cause.exitCode": (p) => `${p.container} 反覆崩潰、重新啟動。`,
  "sentence.diagnosis.crashLoop.cause.exitsZero": (p) =>
    `${p.container} 反覆正常結束後又重新啟動。`,
  "sentence.diagnosis.crashLoop.cause.notExecutable": (p) =>
    `${p.container} 無法執行它的指令：這個檔案不能執行。`,
  "sentence.diagnosis.crashLoop.cause.commandNotFound": (p) =>
    `${p.container} 無法執行它的指令：映像檔裡沒有這個指令。`,
  "sentence.diagnosis.crashLoop.cause.killed": (p) => `${p.container} 反覆被強制終止、重新啟動。`,
  "sentence.diagnosis.crashLoop.cause.terminated": (p) =>
    `${p.container} 反覆收到停止要求後重新啟動。`,
  "sentence.diagnosis.crashLoop.fix.exitsZero": () =>
    "服務必須持續執行；一次性的工作改用 Job 執行。",
  "sentence.diagnosis.crashLoop.fix.command": () => "檢查映像檔的 entrypoint、command 與 args。",
  "sentence.diagnosis.crashLoop.fix.killed": () => "有東西終止了它：檢查記憶體用量與存活檢查。",
  "sentence.diagnosis.crashLoop.fix.terminated": () => "檢查存活檢查，以及是什麼讓程式停止。",
  "sentence.diagnosis.crashLoop.fix.logs": () => "錯誤訊息在重新啟動前那一次的日誌裡。",
  "sentence.diagnosis.runFailed.fix.logs": () => "錯誤訊息在失敗那次執行的日誌裡。",
  "sentence.diagnosis.runFailed.cause.exitCode": (p) => `${p.container} 上一次執行失敗。`,
  "sentence.diagnosis.runFailed.cause.exitsZero": (p) => `${p.container} 上一次執行意外結束。`,
  "sentence.diagnosis.runFailed.cause.notExecutable": (p) =>
    `${p.container} 上一次執行無法啟動指令：這個檔案不能執行。`,
  "sentence.diagnosis.runFailed.cause.commandNotFound": (p) =>
    `${p.container} 上一次執行在映像檔裡找不到指令。`,
  "sentence.diagnosis.runFailed.cause.killed": (p) => `${p.container} 上一次執行被強制終止。`,
  "sentence.diagnosis.runFailed.cause.terminated": (p) =>
    `${p.container} 上一次執行收到停止要求而結束。`,
  "sentence.diagnosis.outOfMemory.cause.limit": (p) =>
    `${p.container} 用完了記憶體，系統終止了它。`,
  "sentence.diagnosis.outOfMemory.cause.noLimit": (p) =>
    `機器的記憶體不足，系統終止了 ${p.container}。`,
  "sentence.diagnosis.outOfMemory.fix.limit": () => "調高記憶體上限，或找出用掉記憶體的地方。",
  "sentence.diagnosis.outOfMemory.fix.noLimit": () => "依實際用量設定記憶體的保留量與上限。",
  "sentence.diagnosis.imagePull.cause.notFound": (p) =>
    `${p.container} 無法下載映像檔 ${p.image ?? ""}：映像檔倉庫裡沒有這個映像檔。`,
  "sentence.diagnosis.imagePull.cause.notFoundOrPrivate": (p) =>
    `${p.container} 無法下載映像檔 ${p.image ?? ""}。`,
  "sentence.diagnosis.imagePull.cause.unauthorized": (p) =>
    `${p.container} 無法下載映像檔：映像檔倉庫拒絕了登入資訊。`,
  "sentence.diagnosis.imagePull.cause.rateLimited": (p) =>
    `${p.container} 無法下載映像檔：超過映像檔倉庫的下載次數限制。`,
  "sentence.diagnosis.imagePull.cause.unreachable": (p) =>
    `${p.container} 無法下載映像檔：連不到映像檔倉庫。`,
  "sentence.diagnosis.imagePull.cause.invalidName": (p) =>
    `${p.container} 的映像檔名稱 ${p.image ?? ""} 有誤。`,
  "sentence.diagnosis.imagePull.cause.unknown": (p) =>
    `${p.container} 無法下載映像檔 ${p.image ?? ""}。`,
  "sentence.diagnosis.imagePull.fix.notFound": () => "確認映像檔的名稱與版本標籤。",
  "sentence.diagnosis.imagePull.fix.notFoundOrPrivate": () =>
    "確認名稱與版本標籤；如果是私有映像檔，替專案加上映像檔倉庫的登入資訊。",
  "sentence.diagnosis.imagePull.fix.unauthorized": () =>
    "替專案加上映像檔倉庫的登入資訊（imagePullSecret），或修正現有的。",
  "sentence.diagnosis.imagePull.fix.rateLimited": () => "登入映像檔倉庫，或改用鏡像站。",
  "sentence.diagnosis.imagePull.fix.unreachable": () => "檢查機器的網路與 DNS。",
  "sentence.diagnosis.imagePull.fix.invalidName": () => "修正映像檔名稱。",
  "sentence.diagnosis.imagePull.fix.unknown": () => "副本事件裡有映像檔倉庫的回應。",
  "sentence.diagnosis.probeFailure.cause.liveness": (p) =>
    `${p.container} 的存活檢查失敗 ${p.count} 次${p.output ? `：${p.output}` : "。"}`,
  "sentence.diagnosis.probeFailure.cause.readiness": (p) =>
    `${p.container} 的就緒檢查失敗 ${p.count} 次${p.output ? `：${p.output}` : "。"}`,
  "sentence.diagnosis.probeFailure.cause.startup": (p) => `${p.container} 沒有在時限內完成啟動。`,
  "sentence.diagnosis.probeFailure.fix.liveness": () =>
    "到健康檢查確認存活檢查的路徑、連接埠與時間設定。",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "到健康檢查確認就緒檢查的路徑、連接埠與時間設定。",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "如果只是啟動得比較慢，到健康檢查放寬時間；如果早該啟動好了，日誌會說明原因。",
  "sentence.gap.resourceLimits.none": (p) =>
    `${p.container} 沒有設定 CPU 與記憶體的 request 或 limit。`,
  "sentence.gap.resourceLimits.partial": (p) =>
    `${p.container} 沒有設定 ${(p.missing ?? "").split(",").join("、")}。`,
  "sentence.gap.healthChecks.none": (p) =>
    `${p.container} 沒有就緒檢查，還沒準備好就可能收到流量。`,
  "sentence.gap.healthChecks.livenessOnly": (p) => `${p.container} 有存活檢查，但沒有就緒檢查。`,
  "sentence.gap.ingressToHttpRoute.retired": (p) =>
    `Ingress ${p.ingress} 使用 ${p.class}，它的 controller 已停止維護。`,
  "sentence.gap.ingressToHttpRoute.available": (p) =>
    `叢集已有 Gateway API，Ingress ${p.ingress} 可以改用 HTTPRoute。`,
  "sentence.gap.retiredIngressController": (p) =>
    `IngressClass ${p.class} 的 controller ${p.controller} 已停止維護。`,
  "sentence.gap.danglingRoute.noService": (p) =>
    `${p.kind} ${p.route} 指定的 Service ${(p.services ?? "").split(",").join("、")} 不在這個專案裡。`,
  "sentence.gap.danglingRoute.noGateway": (p) =>
    `HTTPRoute ${p.route} 指定的 Gateway ${(p.gateways ?? "").split(",").join("、")} 不存在。`,
  "sentence.gap.danglingRoute.noParent": (p) =>
    `HTTPRoute ${p.route} 沒有指定 Gateway，所以它收不到任何流量。`,
  "sentence.gap.danglingRoute.notAccepted": (p) =>
    `沒有任何 Gateway 接受 HTTPRoute ${p.route}${p.reason ? `（${p.reason}）` : ""}，所以它收不到任何流量。`,
  "sentence.gap.blockedConnection": (p) =>
    p.at === "egress"
      ? `${p.from} 的環境變數指向 ${p.to}，但 ${p.from} 的 NetworkPolicy 不允許連出。`
      : p.at === "both"
        ? `${p.from} 的環境變數指向 ${p.to}，但兩端的 NetworkPolicy 都封鎖了這條連線。`
        : `${p.from} 的環境變數指向 ${p.to}，但 ${p.to} 的 NetworkPolicy 不允許 ${p.from} 連入。`,
  "sentence.gap.noNetworkPolicy": (p) => {
    const blocked = cuts(
      p,
      {
        otherNamespaces: "來自其他 Namespace 的連線",
        outside: "連到叢集外的連線",
        published: "NodePort 與 LoadBalancer 的流量",
        connections: "服務之間的一條連線",
      },
      (items) => items.join("、"),
    )
    return [
      `沒有 NetworkPolicy 保護 ${(p.services ?? "").split(",").join("、")}：叢集裡任何 Pod 都連得到。`,
      p.enforcement === "unknown" ? "NetworkPolicy 要網路外掛（CNI）支援才會生效。" : "",
      blocked ? `隔離後也會封鎖${latinStart(blocked)}${blocked}。` : "",
    ].join("")
  },
  "sentence.gap.volumeWithoutBackup": (p) => `磁碟區 ${p.claim} 沒有備份排程。`,
  "sentence.gap.databaseWithoutBackup": (p) => `資料庫 ${p.database} 沒有備份排程。`,
  "sentence.page.noAnswer": () => "Zeabur 沒有回應。",
  "sentence.page.status": (p) => `請求失敗，HTTP 狀態碼 ${p.status}。`,
  "sentence.page.reload": () => "確認 Zeabur 仍在執行，再重新整理頁面。",
  "sentence.page.followFailed": () => "無法持續追蹤日誌。",
  "sentence.page.followOff": () => "關閉「持續追蹤」，快照會顯示叢集的回應。",
  "status.readyUnchecked": "執行中",
  "origin.code": "應用程式本身",
  "origin.configuration": "服務設定",
  "origin.cluster": "叢集",
  "origin.upstream": "叢集外部",
  "origin.unknown": "其他",
  "origin.code.hint": "應用程式本身的程式碼或指令讓它停止；日誌會說明原因。",
  "origin.configuration.hint": "這個服務的某項設定造成問題：變數、上限、檢查或位址。",
  "origin.cluster.hint": "服務底下的叢集造成問題：節點、配額或剩餘資源。",
  "origin.upstream.hint": "叢集外部的東西造成問題，例如映像檔倉庫。",
  "origin.unknown.hint": "Zeabur 還沒把它歸到已知的原因；上面的句子寫著叢集回報的狀態。",
  "fixer.you": "你可以修正",
  "fixer.clusterAdmin": "需要叢集管理員",
  "fixer.unknown": "不確定由誰修正",
  "diagnosis.copy": "複製診斷",
  "diagnosis.copyFailed": "無法複製診斷，請再試一次。",
  "diagnosis.lastSeen": "最後出現",
  "trace.title": "連線檢查",
  "trace.reachable": "可連線",
  "trace.unreachable": "無法連線",
  "trace.unverified": "無法從叢集內確認",
  "trace.howToFix": "修正方式",
  "trace.showChecks": "顯示連線檢查",
  "trace.hop.ok": "通過",
  "trace.hop.failed": "失敗",
  "trace.hop.unknown": "未檢查",
  "status.uncheckedWhy": "容器已經啟動，但沒有檢查它能不能回應請求。",
  "status.uncheckedHint": "沒有設定就緒檢查，所以「執行中」只代表容器已啟動，不代表已能處理請求。",
  "status.addTrafficCheck": "新增就緒檢查",
  "status.private": "私有",
  "rollout.title": "部署進度",
  "rollout.ready": (ready, desired) => `${desired} 個新副本中 ${ready} 個就緒`,
  "rollout.older": (n) => `${n} 個舊副本仍在執行`,
  "rollout.stuckLine": "部署卡住",
  "rollout.waitsLine": "新版本要等這個問題解決",
  "rollout.rollingOut": (name) => `${name} 正在部署`,
  "overview.alsoMeans": "連帶影響",
  "overview.alsoDown": (list, name) => `${name} 有就緒的副本之前，無法連線到 ${list}。`,
  "overview.privateAddress": "私有位址",
  "trace.waiting": (name) => `等待 ${name} 就緒`,
  "trace.stopped": (name) => `${name} 目前是 0 個副本`,
  "event.word.retrying": "重試等待中",
  "event.word.checkFailed": "檢查未通過",
  "event.word.stopping": "正在停止",
  "event.word.failed": "失敗",
  "event.word.noRoom": "等待可用資源",
  "event.word.mountFailed": "無法掛載磁碟區",
  "event.word.createFailed": "無法建立副本",
  "event.word.gaveUp": "已放棄重試",
  "event.word.evicted": "已移出節點",
  "event.word.outOfMemory": "記憶體不足",
  "changes.title": "歷次部署",
  "changes.description":
    "服務的每個發布版本，以及前後發生的警告。可以把某個版本和目前的版本比較，或回滾到該版本。",
  "changes.none": "還沒有部署紀錄",
  "changes.release": (revision) => `修訂版本 #${revision}`,
  "settings.sections": "這個服務的設定",
  "part.variables": "環境變數",
  "part.networking": "網路",
  "part.storage": "儲存空間",
  "part.compute": "運算資源",
  "part.health-checks": "健康檢查",
  "form.healthHint":
    "叢集在每個副本上執行的檢查：能不能接收流量、要不要重新啟動，以及是否已經啟動。",
  "draft.add": "加入變更",
  "draft.pendingHere": "有等待預覽的變更",
  "draft.count": (n) => `${n} 項變更待套用`,
  "draft.review": "預覽變更",
  "draft.none": "尚無變更",
  "draft.save": "儲存",
  "draft.leaveTitle": "不儲存就離開？",
  "draft.leaveBody": (n: number) => `有 ${n} 項變更還沒套用，離開後會捨棄。`,
  "draft.stay": "留下",
  "draft.leave": "捨棄並離開",
  "draft.discard": "捨棄",
  "draft.title": (name) => `變更 ${name}`,
  "project.view": "檢視方式",
  "project.viewMap": "架構圖",
  "project.viewList": "清單",
  "project.suggestions": (n) => `${n} 項建議`,
  "project.suggestionsBody":
    "這些服務目前缺少的設定。它們不會讓服務現在停止，但補上之後，問題更容易被發現或避免。",
  "project.activity": "動態",
  "project.next.title": "部署第一個服務",
  "project.next.body":
    "只要這個命名空間裡有工作負載在執行，服務就會出現在這裡：Deployment、StatefulSet、DaemonSet、Job 或 CronJob。",
  "project.next.command": "用範例網頁伺服器試試看",
  "project.next.after": "用 kubectl 執行，或套用你自己的 YAML。頁面會自動更新。",
  "freshness.failed": "無法更新",
  "freshness.justNow": "剛剛更新",
  "freshness.ago": (count, unit) =>
    `${count} ${unit === "minute" ? "分鐘" : unit === "hour" ? "小時" : "天"}前更新`,
  "freshness.refresh": "重新整理",
  "home.state.fine": (services, projects) =>
    `${projects} 個專案中的 ${services} 個服務都在正常執行。`,
  "home.state.problems": (n) => `有 ${n} 個問題需要處理。`,
  "home.state.rolling": (n) => `${n} 個服務正在部署。`,
  "home.affects": "影響",
  "home.inProgress": "進行中",
  "home.problems": "需要你處理",
  "shell.context": (cluster, user) => `叢集 ${cluster}，登入身分 ${user}`,
  "home.clusterNotices": (n) => `${n} 則叢集提醒，目前沒有服務因此停擺：`,
  "origin.component": "叢集元件",
  "origin.component.hint": "叢集替所有專案執行的元件造成問題，例如 Ingress 控制器或叢集 DNS。",
  "origin.zeabur": "Zeabur 本身",
  "origin.zeabur.hint": "Zeabur 本身造成問題；請附上複製的診斷回報。",
  "fixer.componentOwner": "需要元件負責人",
  "incident.title": "叢集",
  "incident.hint": "一個原因讓好幾個服務停擺；修好它，這些服務就會一起恢復。",
  "incident.domains": (n) => `${n} 個網域`,
  "incident.causedBy": "起因",
  "timeline.truncated": "只顯示最新的 200 筆變更，更早的沒有列出。",
  "timeline.empty": "最近沒有變更。叢集只保留約一小時的事件，以及每個工具最後一次的寫入。",
  "timeline.by": (who) => `由 ${who}`,
  "timeline.tool.zeabur": "Zeabur",
  "timeline.tool.helm": "Helm",
  "timeline.tool.argoCd": "Argo CD",
  "timeline.tool.flux": "Flux",
  "timeline.tool.kubectl": "kubectl",
  "timeline.tool.autoscaler": "自動擴縮",
  "trace.step.dns": "DNS",
  "trace.step.address": "入口位址",
  "trace.step.controller": "Ingress 控制器",
  "trace.step.certificate": "憑證",
  "trace.step.route": "路由",
  "trace.step.service": "私有位址",
  "trace.step.targetPort": "連接埠",
  "trace.step.ready": "就緒副本",
  "trace.expected": "預期",
  "trace.saw": "實際",
  "trace.fix": "修正",
  "trace.fixTitle": (address) => `修正通往 ${address} 的路徑`,
  "rollout.rollBack": (name) => `回滾 ${name}`,
  "rollout.rollBackButton": "回滾",
  "sentence.incident.componentDown.cause.ingressController": (p) =>
    `Ingress 控制器 ${p.workload} 停擺了：它沒有任何就緒的副本。`,
  "sentence.incident.componentDown.cause.certManager": (p) =>
    `cert-manager 停擺了：${p.workload} 沒有任何就緒的副本。`,
  "sentence.incident.componentDown.cause.clusterDns": (p) =>
    `叢集 DNS 停擺了：${p.workload} 沒有任何就緒的副本。`,
  "sentence.incident.componentDown.cause.metricsServer": (p) =>
    `metrics-server 停擺了：${p.workload} 沒有任何就緒的副本。`,
  "sentence.incident.componentDown.why.ingressController": (p) =>
    `公開網域的請求由 Ingress 控制器轉給服務；它停擺時，${p.domains} 個網域和 ${p.services} 個服務收不到叢集外部的請求。`,
  "sentence.incident.componentDown.why.certManager": (p) =>
    `cert-manager 負責簽發和更新網域要求的憑證；它停擺時，${p.domains} 個網域的憑證不會簽發也不會更新，憑證一過期，瀏覽器就會拒絕。`,
  "sentence.incident.componentDown.why.clusterDns": (p) =>
    `服務之間靠叢集 DNS 用名稱找到彼此；它停擺時，有 ${p.services} 個服務用名稱連線，會連不到對方。`,
  "sentence.incident.componentDown.why.metricsServer": () =>
    "metrics-server 負責量測每個副本的用量；它停擺時，看不到用量數字，自動擴縮也會停止。",
  "sentence.incident.componentDown.fix.ingressController": (p) =>
    `${p.workload}${p.namespace ? `（${p.namespace}）` : ""}沒有就緒的副本，從它的事件和日誌找出原因。`,
  "sentence.incident.componentDown.fix.certManager": (p) =>
    `${p.workload}${p.namespace ? `（${p.namespace}）` : ""}沒有就緒的副本，從它的事件和日誌找出原因。`,
  "sentence.incident.componentDown.fix.clusterDns": (p) =>
    `${p.workload}${p.namespace ? `（${p.namespace}）` : ""}沒有就緒的副本，從它的事件和日誌找出原因。`,
  "sentence.incident.componentDown.fix.metricsServer": (p) =>
    `${p.workload}${p.namespace ? `（${p.namespace}）` : ""}沒有就緒的副本，從它的事件和日誌找出原因。`,
  "sentence.incident.nodeNotReady.cause": (p) =>
    `${p.status === "Unknown" ? `節點 ${p.node} 不再向叢集回報狀態` : `節點 ${p.node} 未就緒`}${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.incident.nodeNotReady.why": (p) =>
    `節點未就緒時不會再啟動新的 Pod；寬限期（預設 5 分鐘）過後，叢集會移除上面的 Pod，讓它們到別處啟動。這個節點上有 ${p.pods} 個 Pod，${p.services} 個服務有副本在上面。`,
  "sentence.incident.nodeNotReady.fix": (p) =>
    `檢查 ${p.node} 背後的機器：是否開機、網路，以及節點代理程式（kubelet）。如果它無法恢復，就清空這個節點並換掉它。`,
  "sentence.trace.dns.ok": (p) => `${p.host} 可以解析。`,
  "sentence.trace.dns.notFound": (p) =>
    `從叢集內解析不到 ${p.host}；其他地方的 DNS 可能仍有回應，所以 Zeabur 無法判斷。`,
  "sentence.trace.dns.unknown": (p) => `沒有查詢 ${p.host}，或查詢沒有及時回應。`,
  "sentence.trace.address.noStatus": (p) => `${p.route} 的狀態裡還沒有位址，無法和網域的位址比對。`,
  "sentence.trace.address.unresolved": (p) => `網域沒有解析出位址，無法和 ${p.route} 的位址比對。`,
  "sentence.trace.address.ok": (p) => `網域指向 ${p.route} 提供服務的位址。`,
  "sentence.trace.address.elsewhere": (p) =>
    `網域指向的位址不屬於 ${p.route}；如果叢集前面有 CDN 或代理伺服器，回應的會是它們的位址。`,
  "sentence.trace.controller.classesUnreadable": (p) =>
    `Zeabur 無法讀取叢集的 IngressClass，因此無法判斷哪個控制器負責 ${p.route}。`,
  "sentence.trace.controller.classMissing": (p) =>
    `${p.route} 指定的 IngressClass ${p.class} 不在叢集裡。`,
  "sentence.trace.controller.unreadable": (p) =>
    `Zeabur 無法讀取叢集的元件，因此無法判斷 ${p.route} 的 Ingress 控制器是否在執行。`,
  "sentence.trace.controller.notRecognised": (p) =>
    `Zeabur 認不出負責 ${p.route} 的 Ingress 控制器${p.controller ? ` ${p.controller}` : ""}，因此無法判斷它是否在執行。`,
  "sentence.trace.controller.ok": (p) => `Ingress 控制器 ${p.workload} 有就緒的副本。`,
  "sentence.trace.controller.down": (p) =>
    `Ingress 控制器 ${p.workload} 沒有就緒的副本，所以沒有東西處理 ${p.route}。`,
  "sentence.trace.certificate.defaultCertificate": (p) =>
    `${p.route} 為 ${p.host} 啟用 HTTPS，卻沒有指定機密，所以控制器會提供它的預設憑證。`,
  "sentence.trace.certificate.ok": (p) =>
    `${p.host} 的憑證已簽發${p.until ? `，有效期限到 ${p.until}` : ""}。`,
  "sentence.trace.certificate.notReady": (p) =>
    `${p.host} 的憑證 ${p.certificate} 尚未簽發${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.trace.certificate.pending": (p) => `${p.host} 的憑證 ${p.certificate} 還沒有結果。`,
  "sentence.trace.certificate.unreadable": (p) =>
    `Zeabur 無法讀取專案的機密，因此無法檢查機密 ${p.secret} 裡 ${p.host} 的憑證。`,
  "sentence.trace.certificate.notChecked": (p) =>
    `機密 ${p.secret} 存有 ${p.host} 的憑證；Zeabur 不會進一步檢查。`,
  "sentence.trace.certificate.secretMissing": (p) =>
    `${p.route} 從機密 ${p.secret} 讀取 ${p.host} 的憑證，但專案裡沒有這個機密；控制器可能改用預設憑證，Zeabur 無法檢查它。`,
  "sentence.trace.route.noStatus": (p) =>
    `還沒有任何元件寫入 ${p.route} 的狀態，無法得知它是否已生效。`,
  "sentence.trace.route.ok": (p) => `${p.route} 已生效：控制器已在它的狀態裡寫入位址。`,
  "sentence.trace.route.accepted": (p) => `Gateway 已接受 ${p.route}。`,
  "sentence.trace.route.notAccepted": (p) =>
    `${p.route} 的流量入口回報 ${p.condition} ${p.reason}；請求無法通過。`,
  "sentence.trace.service.unreadable": (p) =>
    `Zeabur 無法讀取專案的私有位址（Service），因此無法檢查 ${p.endpoint}。`,
  "sentence.trace.service.missing": (p) => `找不到名為 ${p.endpoint} 的私有位址（Service）。`,
  "sentence.trace.service.portMissing": (p) => `私有位址 ${p.endpoint} 沒有連接埠 ${p.port}。`,
  "sentence.trace.service.selectsOther": (p) =>
    `私有位址 ${p.endpoint} 把流量送到其他副本，而不是 ${p.service} 的副本。`,
  "sentence.trace.service.ok": (p) => `私有位址 ${p.endpoint} 把流量送到 ${p.service}。`,
  "sentence.trace.targetPort.unknown": (p) =>
    `Zeabur 讀不到容器，因此沒有檢查 ${p.endpoint} 轉送到的連接埠。`,
  "sentence.trace.targetPort.notReached": (p) =>
    `沒有檢查：流量在到達 ${p.endpoint} 的連接埠之前就停了。`,
  "sentence.trace.targetPort.mismatch": (p) =>
    `${p.endpoint} 把流量送到連接埠 ${p.targetPort}，容器卻沒有宣告它${p.ports ? `；容器宣告的是 ${p.ports}` : ""}。`,
  "sentence.trace.targetPort.undeclared": (p) =>
    `容器沒有宣告任何連接埠，無法得知它們是否在 ${p.endpoint} 轉送到的連接埠上監聽。`,
  "sentence.trace.targetPort.ok": (p) => `${p.endpoint} 把流量送到容器宣告的連接埠。`,
  "sentence.trace.ready.scaledToZero": (p) => `${p.service} 的副本數設為 0。`,
  "sentence.trace.ready.ok": (p) => `${p.service} 有 ${p.ready}/${p.pods} 個副本就緒。`,
  "sentence.trace.ready.starting": (p) => `${p.service} 正在啟動，還沒有就緒的副本。`,
  "sentence.trace.ready.noneReady": (p) => `${p.service} 沒有任何就緒的副本。`,
  "sentence.diagnosis.unreachable.cause.ingressClassMissing": (p) =>
    `送到 ${p.route} 網域的請求到不了 ${p.service}：它指定的 IngressClass ${p.class} 不在叢集裡。`,
  "sentence.diagnosis.unreachable.why.ingressClassMissing": () =>
    "Ingress 控制器只處理自己類別的 Ingress；沒有這個類別，就沒有控制器處理它。",
  "sentence.diagnosis.unreachable.fix.ingressClassMissing": (p) =>
    `把 ${p.route} 改用叢集裡現有的類別，或安裝 ${p.class} 的控制器。`,
  "sentence.diagnosis.unreachable.cause.controllerDown": (p) =>
    `送到 ${p.route} 網域的請求到不了 ${p.service}：Ingress 控制器 ${p.workload} 沒有就緒的副本。`,
  "sentence.diagnosis.unreachable.why.controllerDown": () =>
    "Ingress 控制器負責把網域的請求帶進叢集；它沒有任何就緒的副本時，就沒有東西回應這些請求。",
  "sentence.diagnosis.unreachable.fix.controllerDown": (p) =>
    `${p.workload}${p.namespace ? `（${p.namespace}）` : ""}沒有就緒的副本，從它的事件和日誌找出原因。`,
  "sentence.diagnosis.unreachable.cause.certificateNotReady": (p) =>
    `${p.host} 的 HTTPS 連線失敗：憑證 ${p.certificate} 尚未簽發${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.diagnosis.unreachable.why.certificateNotReady": (p) =>
    `cert-manager 簽發憑證後才會把它寫進機密 ${p.secret}；在那之前，控制器會改用預設憑證，瀏覽器會拒絕它。`,
  "sentence.diagnosis.unreachable.fix.certificateNotReady": (p) =>
    `憑證 ${p.certificate} 的狀態和事件會說明它為什麼還沒簽發。`,
  "sentence.diagnosis.unreachable.cause.backendPortMissing": (p) =>
    `通往 ${p.service} 的路由指定了連接埠 ${p.port}，但它的私有位址 ${p.endpoint} 沒有這個連接埠。`,
  "sentence.diagnosis.unreachable.why.backendPortMissing": () =>
    "路由會用編號或名稱把流量送到私有位址的某個連接埠；私有位址沒有的連接埠收不到任何流量。",
  "sentence.diagnosis.unreachable.fix.backendPortMissing": (p) =>
    `讓路由指向 ${p.endpoint} 有的連接埠，或在 ${p.endpoint} 加上連接埠 ${p.port}。`,
  "sentence.diagnosis.unreachable.cause.selectsOther": (p) =>
    `通往 ${p.service} 的路由把流量送到 ${p.endpoint}，但 ${p.endpoint} 會送到其他副本，而不是 ${p.service} 的副本。`,
  "sentence.diagnosis.unreachable.why.selectsOther": () =>
    "私有位址只會把流量送到標籤符合它 selector 的副本。",
  "sentence.diagnosis.unreachable.fix.selectsOther": (p) =>
    `修改 ${p.endpoint} 的 selector，讓它符合 ${p.service} 的標籤，或讓路由改指向 ${p.service} 自己的私有位址。`,
  "sentence.diagnosis.unreachable.cause.scaledToZero": (p) =>
    `沒有流量到得了 ${p.service}：它的副本數設為 0。`,
  "sentence.diagnosis.unreachable.why.scaledToZero": () =>
    "私有位址只會把流量送到執行中的副本，而目前設定不執行任何副本。",
  "sentence.diagnosis.unreachable.fix.scaledToZero": (p) => `把 ${p.service} 的副本數調到至少 1。`,
  "sentence.timeline.release": (p) =>
    `修訂版本 #${p.revision}${p.images ? ` 使用 ${p.images}` : ""}${p.changeCause ? `（${p.changeCause}）` : ""}。`,
  "sentence.timeline.write.zeabur": (p) =>
    `Zeabur 最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.helm": (p) => `Helm 最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.argoCd": (p) =>
    `Argo CD（${p.manager}）最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.flux": (p) =>
    `Flux（${p.manager}）最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.kubectl": (p) =>
    `kubectl（${p.manager}）最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.autoscaler": () =>
    "自動擴縮（HorizontalPodAutoscaler）最近一次修改了副本數。",
  "sentence.timeline.write.controller": (p) =>
    `Kubernetes（${p.manager}）最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.other": (p) =>
    `${p.manager} 最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.warning.crashLoop": (p) => `${p.object} 反覆崩潰、重新啟動（${p.count} 次）。`,
  "sentence.timeline.warning.evicted": (p) =>
    `${p.object} 被移出所在的節點（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.unschedulable": (p) =>
    `${p.object} 找不到可以執行的機器（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.imagePull": (p) =>
    `${p.object} 無法下載映像檔（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.probe": (p) =>
    `${p.object} 的健康檢查失敗（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.quota": (p) =>
    `${p.object} 無法建立副本（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.other": (p) =>
    `${p.object} 發生 ${p.reason}（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.termination.oomKilled": (p) =>
    `${p.pod} 裡的 ${p.container} 用完了記憶體，系統終止了它。`,
  "sentence.timeline.termination.exited": (p) =>
    `${p.pod} 裡的 ${p.container} 結束了，結束碼 ${p.exitCode}${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.rollout.progressing": (p) =>
    `${p.name} 正在部署：${p.desired} 個副本中有 ${p.updated} 個是新的修訂版本。`,
  "sentence.rollout.done": (p) => `${p.name} 已部署完成：每個副本都是目前的修訂版本。`,
  "sentence.rollout.paused": (p) => `${p.name} 的部署已暫停；恢復後才會啟動新的副本。`,
  "sentence.rollout.waitsForDeletion": (p) =>
    `${p.name} 只在副本被刪除時才換成新版本（OnDelete）；${p.desired} 個中有 ${p.updated} 個是目前的修訂版本。`,
  "sentence.rollout.stuck.crashLoop": (p) => `${p.name} 的部署卡住了：新副本反覆崩潰。`,
  "sentence.rollout.stuck.outOfMemory": (p) => `${p.name} 的部署卡住了：新副本用完了記憶體。`,
  "sentence.rollout.stuck.imagePull": (p) => `${p.name} 的部署卡住了：新副本無法下載映像檔。`,
  "sentence.rollout.stuck.unschedulable": (p) =>
    `${p.name} 的部署卡住了：新副本正在等待可以執行的機器。`,
  "sentence.rollout.stuck.quota": (p) => `${p.name} 的部署卡住了：專案用量上限不允許新增副本。`,
  "sentence.rollout.stuck.configMissing": (p) =>
    `${p.name} 的部署卡住了：新副本讀取的機密或設定檔不存在。`,
  "sentence.rollout.stuck.runFailed": (p) => `${p.name} 的部署卡住了：新副本執行失敗。`,
  "sentence.rollout.stuck.deadlineExceeded": (p) =>
    `${p.name} 的部署卡住了：期限（progressDeadlineSeconds）內沒有進展。`,
  "sentence.rollout.stuck.replicaFailure": (p) => `${p.name} 的部署卡住了：叢集無法建立新副本。`,
  "sentence.gap.ephemeralData.none": (p) =>
    `${p.container} 把 ${p.software} 資料存在 ${p.path}，但這個路徑不在任何磁碟區上；副本一換掉，資料就不見了。`,
  "sentence.gap.ephemeralData.ephemeral": (p) =>
    `${p.container} 把 ${p.software} 資料存在 ${p.path}，但這個磁碟區只跟著副本存在；副本一換掉，資料就不見了。`,
  // ia:start
  "preview.restarts": "會重新啟動的服務",
  "preview.roll.rolling": (name) => `${name} 會逐一替換副本。`,
  "preview.roll.recreate": (name) => `${name} 會先停止所有副本，再啟動新的。`,
  "preview.roll.onDelete": (name) => `${name} 要等你刪除舊副本後才會套用新設定。`,
  "preview.scale": (name, from, to) => `${name} 從 ${from} 個副本變成 ${to} 個。`,
  "preview.start": (name) => `${name} 會啟動。`,
  "preview.stop": (name) => `${name} 會停止。`,
  "preview.pendingRestart": (name) => `${name} 重新啟動前會繼續使用舊的值。`,
  "preview.downtime": "會有短暫的時間沒有副本提供服務。",
  "preview.cores": (n) => `${n} 核心 CPU`,
  "preview.fits": (pods, cpu, memory) =>
    `資源足夠：尖峰時會多出 ${pods} 個 Pod、${cpu} 和 ${memory}。`,
  "preview.blocked": "資源不足的原因",
  "preview.blockedQuota": (name, resource, need, left) =>
    `配額 ${name} 不夠：${resource} 需要 ${need}，只剩 ${left}。`,
  "preview.blockedNodes": (_name, resource, need, left) =>
    `沒有節點放得下：${resource} 需要 ${need}，最空的節點只有 ${left}。`,
  "preview.fitUnknown": "Zeabur 無法確認新的 Pod 是否放得下。",
  "preview.dataLoss": "會遺失的資料",
  "preview.loss.claimDeleted": (claim, paths) =>
    `會刪除磁碟 ${claim}${paths ? `，以及 ${paths} 裡的資料` : ""}。`,
  "preview.loss.emptyDir": (name, volume, paths) =>
    `${name} 把 ${paths || volume} 放在暫存磁碟區；替換副本時會清空。`,
  "preview.loss.statefulSet": (name, claim) =>
    `${name} 會刪除從 ${claim} 建立的磁碟，資料也會一起刪除。`,
  "preview.removals": "會移除的設定",
  "preview.removal": (path, name) => `會從 ${name} 移除 ${path}，目前是`,
  "preview.leftOut": "（你編輯時沒有保留）",
  "env.pendingRestart": "重新啟動前可能仍在使用舊的值",
  "env.pendingRestartDetail": (pod) => `這個值在 ${pod} 啟動後才變更。重新啟動服務才會使用新的值。`,
  // ia:end
}
