import type { Messages } from "@/i18n/en"
import { cuts, listZh, probeTiming, thingZh, type P, type ProbeTiming } from "@/i18n/sentence-parts"

// A space where Han text meets a Latin word ("查看 Secrets 需要…").

/** How many replicas of the release before still serve, after a stuck rollout's sentence. */
const servingZhCN = (p: P) =>
  Number(p.serving ?? 0) > 0 ? `上一个版本的 ${p.serving} 个副本仍在提供服务。` : ""

const latinStart = (text: string) => (/^[A-Za-z0-9]/.test(text) ? " " : "")
const latinEnd = (text: string) => (/[A-Za-z0-9]$/.test(text) ? " " : "")

/** What a refused verb does, in a phrase: `patch` changes, `create` adds. */
const verbZhCN: Record<string, string> = {
  get: "读取",
  list: "列出",
  watch: "监视",
  create: "新增",
  update: "更改",
  patch: "更改",
  delete: "删除",
}

const objectZhCN = (p: P) => (p.object === "Secret" ? `机密 ${p.name}` : `配置文件 ${p.name}`)

/** Who a volume's mount failure stops: the container, else the replica. */
const mountedZhCN = (p: P) => p.container ?? `副本 ${p.pod}`

/** Seconds in words, whole minutes as minutes. */
const secondsZhCN = (s: number) => (s >= 60 && s % 60 === 0 ? `${s / 60} 分钟` : `${s} 秒`)

/** A wait in its largest unit, rounded up: 90 s is 2 minutes. */
const waitZhCN = (s: number) =>
  s < 60 ? `${s} 秒` : s < 3600 ? `${Math.ceil(s / 60)} 分钟` : `${Math.ceil(s / 3600)} 小时`

/** The sentence, led by "by default" when both values are the defaults, else followed by which one is. */
const timingZhCN = (t: ProbeTiming, sentence: string) =>
  t.defaults
    ? `默认情况下，${sentence}`
    : `${sentence}${t.periodOnly ? `每 ${secondsZhCN(t.period)}是默认值。` : ""}${
        t.thresholdOnly ? `${t.threshold} 次是默认值。` : ""
      }`

export const zhCN: Messages = {
  "shell.skip": "跳到主要内容",
  "shell.nav": "主导航",
  "shell.home": "首页",
  "shell.projects": "项目",
  "shell.settings": "设置",
  "shell.cluster": "集群",
  "shell.search": "搜索",
  "shell.jumpTo": "跳转到…",
  "shell.menu": "菜单",
  "shell.findProject": "查找项目",
  "shell.manage": "管理",
  "shell.whoamiUnknown": "未知用户",
  "shell.signOut": "退出登录",
  "shell.theme": "主题",
  "elevate.retry": "以管理员身份重试",
  "elevate.title": "管理员权限",
  "elevate.waiting": "在新窗口用通行密钥确认。",
  "elevate.notAllowed": "这个 Zeabur 不能请求管理员权限，请联系集群管理员。",
  "elevate.blocked": "浏览器拦截了窗口。允许此网站弹出窗口后，再试一次。",
  "elevate.failed": "没有获得管理员权限。",
  "elevate.signedOut": "会话已结束，重新登录后再继续。",
  "elevate.signIn": "登录",
  "elevate.until": (time) => `管理员权限至 ${time}`,
  "elevate.activeDetail": "在此之前，你的所有操作都会使用管理员权限。",
  "elevate.drop": "放弃管理员权限",
  "elevate.use": "使用管理员权限",
  "shell.switchProject": "切换项目",
  "column.label": "服务",
  "column.other": "其他对象",
  "column.advanced": "高级",
  "column.namespace": "Namespace",

  "theme.system": "跟随系统",
  "theme.light": "亮色",
  "theme.dark": "暗色",

  "common.colon": "：",
  "common.loading": "正在加载…",
  "common.retry": "重试",
  "common.all": "全部",
  "common.close": "关闭",
  "common.copy": "复制",
  "common.copied": "已复制",
  "common.status": "状态",
  "common.sep": "，",
  "common.list": (items) => listZh("zh-CN", items),
  "common.domains": "域名",
  "common.newTab": "（在新标签页打开）",
  "common.notFound": "页面不存在",
  "common.backToProjects": "返回项目",
  "common.managedBy": (tool) => `由 ${tool} 管理`,
  "common.managedByDetail": (tool) => `${tool} 下次同步或升级时，会覆盖所有不经它做的修改。`,

  "problem.whatToDo": "怎么处理",
  "problem.kubernetes": "Kubernetes 详细信息",
  "problem.code": "HTTP 状态码",
  "problem.reason": "原因",
  "problem.message": "消息",
  "problem.title.bad-request": "请求无效",
  "problem.title.unauthenticated": "你尚未登录",
  "problem.title.signed-out": "你已退出登录",
  "problem.title.forbidden": "你没有权限",
  "problem.title.elevation-required": "这需要管理员权限",
  "problem.title.quota-exceeded": "这会超过项目的用量上限",
  "problem.title.policy-denied": "集群里的策略拒绝了这项更改",
  "problem.title.not-found": "找不到这个对象",
  "problem.title.log-unavailable": "无法获取日志",
  "problem.title.conflict": "在此期间已有其他更改",
  "problem.title.managed": "由其他工具管理",
  "problem.title.plan-expired": "计划已过期",
  "problem.title.system-namespace": "这是 Kubernetes 自用的 Namespace",
  "problem.title.invalid": "集群拒绝了这项更改",
  "problem.title.too-many-requests": "集群繁忙",
  "problem.title.unavailable": "无法连接集群",
  "problem.title.internal": "Zeabur 出错了",
  "problem.fix.bad-request": "检查请求后重试。",
  "problem.fix.unauthenticated": "重新登录。",
  "problem.fix.signed-out": "重新登录。",
  "problem.fix.forbidden": "向集群管理员申请权限，或改用有权限的账号。",
  "problem.fix.elevation-required": "以管理员身份重试，或向集群管理员申请权限。",
  "problem.fix.quota-exceeded": "减少请求的量、释放项目已占用的资源，或请集群管理员调高上限。",
  "problem.fix.policy-denied": "按策略指出的地方修改，或向集群管理员询问这项策略。",
  "problem.fix.not-found": "刷新页面，查看集群当前有什么。",
  "problem.fix.log-unavailable": "改看本次运行的日志，或稍后再刷新。",
  "problem.fix.conflict": "刷新页面查看最新版本，再做一次更改。",
  "problem.fix.managed":
    "到管理工具读取配置的地方更改，例如 Helm values 或 Git 仓库。这里仍然可以重启。",
  "problem.fix.plan-expired": "重新生成计划。",
  "problem.fix.system-namespace": "改为纳入你自己的 Namespace。",
  "problem.fix.invalid": "修正详细信息中集群指出的字段，然后重试。",
  "problem.fix.too-many-requests": "稍等片刻再重试。",
  "problem.fix.unavailable": "确认集群正在运行，并且这台机器能连接到它。",
  "problem.fix.internal": "再试一次。如果问题持续出现，请附上详细信息反馈。",
  "problem.detail": "Zeabur 的消息",
  "problem.about": "这类问题的说明",
  "problem.retryAfter": (s) => `集群要求等待 ${waitZhCN(s)}。`,
  "problemPage.label": "问题类型",
  "problemPage.rule": "规则",
  "problemPage.kubernetes": "Kubernetes 文档",
  "problemPage.rule.bad-request":
    "Zeabur 会在请求发到集群之前先检查：要更改的内容必须完整、格式正确。不符合的请求，Zeabur 会直接拒绝，什么都不写入。",
  "problemPage.rule.unauthenticated":
    "每个请求都以你自己的登录身份发到集群。如果集群不再接受这个登录（已过期或已撤销），在你重新登录之前，什么都无法读取或更改。",
  "problemPage.rule.signed-out":
    "你的 Zeabur 会话已结束，Zeabur 手上没有可以代你使用的登录。在你重新登录之前，什么都不会读取或更改。",
  "problemPage.rule.forbidden":
    "集群用规则（RBAC）决定每个人能做什么：谁可以读取或更改哪些类型的对象、在哪些项目里。Zeabur 只用你自己的权限行事，你的权限不允许的事，它也无法代你做。",
  "problemPage.rule.elevation-required":
    "你平常的权限不允许这件事，但这个 Zeabur 可以在你用通行密钥确认后，短时间给你管理员权限。在权限到期或你放弃之前，你的所有操作都会使用它。",
  "problemPage.rule.quota-exceeded":
    "项目可以设置所有服务加起来能预留的上限：CPU、内存、副本或对象的数量。更改如果会超过上限，集群就会拒绝，与你的权限无关。",
  "problemPage.rule.policy-denied":
    "集群可以用自己的策略（由 admission webhook 或策略引擎执行）检查每一项更改。策略拒绝时，它的原话会说明原因；你的权限改变不了它的回应。",
  "problemPage.rule.not-found": "要找的东西不在集群里：页面读取之后，可能有人删除或重命名了它。",
  "problemPage.rule.log-unavailable":
    "集群只保留容器这一次与上一次运行的日志，而且只在副本还存在时保留。从未写出的日志，或副本已经不在的日志，都无法读取。",
  "problemPage.rule.conflict":
    "Zeabur 只会把更改写到你预览的那个版本上。如果之后有人或其他程序（例如部署工具、自动扩缩）改了这个对象，就什么都不会写入，谁的更改都不会丢失。",
  "problemPage.rule.managed":
    "这个对象由其他工具（Helm、Argo CD、Flux）管理。那个工具下一次同步或升级时，会换回自己的版本，覆盖在这里做的更改，所以 Zeabur 不接受。",
  "problemPage.rule.plan-expired":
    "计划就是你预览过、Zeabur 将要写入的内容。它只在几分钟内有效，确保应用时内容仍然符合集群当前的状态。",
  "problemPage.rule.system-namespace":
    "Kubernetes 保留了几个 Namespace 给自己的组件使用（kube-system、kube-public、kube-node-lease），它们不能成为项目。",
  "problemPage.rule.invalid":
    "集群存储对象之前，会按照它的模式与规则检查。集群拒绝了这个对象，并指出了哪些字段无效。",
  "problemPage.rule.too-many-requests":
    "集群会限制同时处理的请求数，以保持响应速度。集群暂时拒绝了这个请求，没有任何更改。",
  "problemPage.rule.unavailable":
    "Zeabur 连不上集群的 API 服务器：它可能停机或正在重启，或从 Zeabur 运行的地方连不上。",
  "problemPage.rule.internal": "出错的是 Zeabur 本身，不是集群。详细信息里有 Zeabur 知道的情况。",
  "refused.admin": "这需要管理员权限。",
  "refused.adminFor": (what) => `${what}${latinEnd(what)}需要管理员权限。`,
  "refused.rights": "这需要更多权限。",
  "refused.rightsFor": (what) => `${what}${latinEnd(what)}需要更多权限。`,
  "refused.ask": "请联系集群管理员。",
  "refused.view": "以管理员身份查看",
  "refused.details": "详情",
  "refused.missing": "缺少的权限",
  "refused.see": (what) => `查看${latinStart(what)}${what}`,
  "refused.some": "部分内容",
  "refused.terminal": "打开终端",
  "refused.reveal": (name) => `显示 ${name} 的值`,
  "refused.listThese": "列出这些对象",
  "refused.adopt": (ns) => `纳入 ${ns}`,
  "refused.unadopt": (ns) => `从 Zeabur 移除 ${ns}`,

  "status.ready": "就绪",
  "status.rollingOut": "部署中",
  "status.failing": "异常",
  "status.degraded": "未全部就绪",
  "status.scheduled": "已调度",
  "status.paused": "已暂停",
  "status.rolloutStuck": "部署卡住",
  "status.failed": "失败",
  "status.scaledToZero": "已缩容到 0",
  "status.completed": "已完成",
  "status.unknown": "未知",
  "status.lastExit": (code, ago) => (ago ? `上次退出码 ${code}，${ago}` : `上次退出码 ${code}`),
  "reason.crashing": "反复崩溃",
  "reason.runFailed": "运行失败",
  "reason.outOfMemory": "内存不足",
  "reason.imagePull": "无法下载镜像",
  "reason.imageName": "镜像名称有误",
  "reason.configMissing": "缺少配置",
  "reason.cantStart": "无法启动",
  "reason.waitingForRoom": "等待可用资源",
  "reason.starting": "正在启动",
  "reason.evicted": "已移出节点",
  "reason.releaseStuck": "部署卡住",
  "reason.gaveUp": "已放弃重试",
  "phase.Running": "运行中",
  "phase.Progressing": "部署中",
  "phase.Degraded": "未全部就绪",
  "phase.Paused": "已暂停",
  "phase.Suspended": "已挂起",
  "phase.Completed": "已完成",
  "phase.Failed": "失败",
  "phase.Unknown": "未知",

  "role.web": "Web",
  "role.internal": "私有服务",
  "role.worker": "后台任务",
  "role.job": "批处理任务",
  "role.data": "数据",

  "metric.restarts": "重启次数",
  "metric.warnings": "警告",
  "metric.services": "服务",

  "home.title": "首页",
  "home.status": "状态",
  "home.rollingOut": (n) => `${n} 个正在部署`,
  "home.nodesReady": (ready, total) => `${ready}/${total} 个节点就绪`,
  "home.allProjects": "所有项目",
  "home.warningsHour": "警告（1 小时）",
  "home.noEventsPermission": "你没有列出集群事件的权限。",
  "home.noServices": "还没有服务",

  "cluster.title": "集群",
  "cluster.tabs": "集群页面",
  "cluster.overview": "概览",
  "cluster.catalogue": "目录",
  "cluster.endOfLife": "已停止支持",
  "cluster.endOfLifeOn": (date) => `${date} 停止支持`,
  "cluster.skewBadge": "节点版本不受支持",
  "cluster.upgrading": "升级中",
  "cluster.available": (version) => `可升级到 ${version}`,
  "cluster.patch": (version) => `补丁版本 ${version}`,
  "cluster.nextMinor": (version) => `下一个次版本 ${version}`,
  "cluster.allClear": "没有需要处理的事项。",
  "cluster.more": (n) => `还有 ${n} 项`,
  "cluster.fewer": "收起",
  "cluster.attEolSoon": (minor, date) => `Kubernetes ${minor} 将于 ${date} 停止支持`,
  "cluster.attEolPast": (minor, date) => `Kubernetes ${minor} 已于 ${date} 停止支持`,
  "cluster.attEolOld": (minor) => `Kubernetes ${minor} 已停止支持`,
  "cluster.attSkew": (node, version) => `${node} 的 kubelet 为 ${version}，超出支持的版本差距`,
  "cluster.attTight": (node, what, percent) =>
    `${node}：${what}${latinEnd(what)}预留量已达 ${percent}`,
  "cluster.attNode": (node, state) => `${node}：${state}`,
  "cluster.attComponent": (name, version, tested) => `${name} ${version} 比已测试的 ${tested} 旧`,
  "cluster.noProjects": "还没有项目",
  "cluster.noProjectsBody": "项目是 Zeabur 显示的 Kubernetes Namespace。在“项目”页面纳入一个。",
  "cluster.openProjects": "打开项目页面",
  "cluster.nodes": "节点",
  "cluster.cpu": "CPU",
  "cluster.memory": "内存",
  "cluster.cores": (n) => `${n} 核`,
  "cluster.requestedOf": (requested, allocatable) => `已预留 ${requested}/${allocatable}`,
  "cluster.allocatable": (allocatable) => `可分配 ${allocatable}`,
  "cluster.used": (used) => `已使用 ${used}`,
  "cluster.limits": (limits) => `上限 ${limits}`,
  "cluster.podsOf": (placed, room) => `${placed}/${room} 个 Pod`,
  "cluster.pods": "Pod",
  "cluster.notReady": "未就绪",
  "cluster.cordoned": "已停止调度",
  "cluster.worker": "worker",
  "cluster.kubelet": "kubelet",
  "cluster.skewMixed": "与控制平面的次版本不同，仍受支持。",
  "cluster.skewUnsupported": "不受支持：比控制平面新，或比它旧超过 3 个次版本。",
  "cluster.noMetrics": "安装 metrics-server 后才能显示用量。",
  "cluster.metricsUnavailable": "metrics-server 没有响应。",
  "cluster.noUsageYet": "还没有测得用量。",
  "cluster.partialPods": (n) => `预留量只计入前 ${n} 个 Pod。`,
  "cluster.unscheduled": (n) => `${n} 个 Pod 等待调度`,
  "cluster.warnings": "最近一小时的警告",
  "home.showWarnings": (n) => `显示 ${n} 条警告`,
  "cluster.noWarnings": "最近一小时没有警告。",
  "cluster.moreWarnings": (n) => `还有 ${n} 条`,
  "cluster.partialEvents": "只读取了最新的 5,000 条事件。",
  "cluster.noComponents": "还没有安装目录中的组件。从下方选择一个，查看安装方法。",
  "cluster.tested": (version) => `已测试 ${version}`,
  "cluster.testedVersion": "已测试版本",
  "cluster.newerThanTested": "比已测试版本新",
  "cluster.sourceK3s": "k3s",
  "cluster.sourceHelmChart": "HelmChart",
  "cluster.sourceHelm": "Helm",
  "cluster.sourceOther": "—",
  "cluster.roleReadsSecrets":
    "这个角色能读取集群里的每个 Secret，包括密码、密钥和令牌。只授予你愿意让其担任集群管理员的人。",
  "cluster.copyRole": "复制 ClusterRole",
  "cluster.whatProjects": "项目",
  "cluster.whatNodes": "节点",
  "cluster.whatRequests": "Pod 的预留量",
  "cluster.whatUpgrades": "升级计划",
  "cluster.whatUsage": "节点用量",
  "cluster.whatWarnings": "集群的警告",
  "cluster.whatComponents": "已安装的组件",

  "projects.title": "项目",
  "projects.adopted": "项目",
  "projects.namespaceOnly": "Namespace",
  "projects.other": "其他 Namespace",
  "projects.noneBody": "项目是 Zeabur 显示的 Kubernetes Namespace。从下方的 Namespace 中纳入一个。",
  "projects.showSystem": "显示系统 Namespace",
  "projects.forbiddenTitle": "你无权列出项目",
  "projects.view": "视图",
  "projects.viewGrid": "卡片",
  "projects.viewList": "列表",

  "project.sections": "项目页面",
  "project.entry.overview": "概览",
  "project.services": "服务",
  "project.events": "事件",
  "project.noServices": "还没有服务",
  "project.seeObjects": "查看所有对象",
  "project.healthyOf": (healthy, total) => `${healthy}/${total} 正常`,

  "attention.title": "需要处理",
  "attention.ready": (ready, desired) => `${ready}/${desired} 就绪`,
  "attention.logsOf": (pod) => `${pod} 的日志`,
  "attention.eventsOf": (pod) => `${pod} 的事件`,
  "overview.updated": "更新于",
  "overview.healthy": "正常",
  "overview.failingCount": (n) => `${n} 个异常`,
  "overview.degradedCount": (n) => `${n} 个未就绪`,
  "events.show": "显示事件",
  "events.warnings": "警告",
  "events.noWarnings": "没有警告。",
  "events.empty": "最近没有事件。Kubernetes 只保留大约一小时内的事件。",
  "events.label": "事件，最新的在前",
  "events.warning": "警告",
  "events.normal": "正常",
  "events.forPod": "副本",
  "events.clearPod": "显示所有副本",
  "domains.https": "已配置 HTTPS",
  "resource.unused": "未使用",
  "resource.unusedCount": (n) => `${n} 个未使用`,
  "storage.requestedTotal": (size) => `共申请 ${size}`,
  "storage.notMounted": "没有服务挂载",

  "table.service": "服务",
  "table.status": "状态",
  "table.restarts": "重启",
  "table.domain": "域名",
  "table.actions": "操作",
  "table.kind": "类型",
  "table.pods": "副本",
  "services.search": "筛选服务",
  "services.attention": "需要处理",
  "services.healthy": "正常",
  "services.allKinds": "所有类型",
  "services.count": (shown, total) => (shown === total ? `${total} 个服务` : `${shown} / ${total}`),
  "services.noMatch": "没有匹配的服务。",

  "lane.domains": "域名",
  "lane.web": "Web",
  "lane.internal": "私有服务",
  "lane.workers": "后台与批处理任务",
  "lane.data": "数据",
  "edge.declared": "声明",
  "edge.inferred": "推断",
  "edge.route": (ingress) => `Ingress ${ingress}`,
  "edge.shared": (kind, name) => `${kind} ${name}`,
  "edge.env": (variable) => variable,
  "canvas.zoomIn": "放大",
  "canvas.zoomOut": "缩小",
  "canvas.fit": "适应画面",
  "canvas.legend": "连线说明",
  "canvas.legendHint":
    "根据 Kubernetes 对象和环境变量值绘制，不是根据实际流量。声明：Ingress，或共用的 Secret、ConfigMap。推断：环境变量值提到了该 Service。",
  "architecture.edges": (n) => `${n} 条连线`,
  "quickLook.label": (name) => `快速查看：${name}`,
  "quickLook.open": "打开服务页面",
  "quickLook.upstream": "上游",
  "quickLook.downstream": "下游",

  "service.tabs": "服务页面",
  "service.more": "更多",
  "service.views": "查看方式",
  "service.delete": "删除服务",
  "service.notFound": "找不到这个服务。",
  "service.created": (date) => `创建于 ${date}`,
  "tab.overview": "概览",
  "tab.logs": "日志",
  "tab.deployments": "部署记录",
  "tab.networking": "网络",
  "tab.variables": "变量",
  "tab.settings": "设置",
  "tab.terminal": "终端",
  "tab.storage": "存储",
  "tab.events": "事件",
  "tab.usage": "用量",
  "ops.restart": "重启",
  "ops.later": "将在后续版本提供。",
  "overview.started": (ago) => `${ago}启动`,
  "overview.source": "来源",
  "overview.cluster": "集群",
  "overview.managedBy": "管理工具",
  "overview.replicas": (ready, desired) => `副本 ${ready}/${desired}`,
  "overview.replicaSettings": "副本设置",
  "overview.noneReady": "没有就绪的副本",
  "overview.someReady": (ready, desired) => `${desired} 个副本中 ${ready} 个就绪`,
  "overview.crashing": (n) => `${n} 个副本反复崩溃`,
  "overview.manage": "管理",
  "overview.warnings": "警告",
  "pod.pods": "副本",
  "pod.none": "目前没有运行中的副本。",
  "pod.ready": "就绪",
  "pod.notReady": "未就绪",
  "pod.readyCount": (ready, total) => `${ready}/${total} 就绪`,
  "pod.restarts": (n) => `重启 ${n} 次`,
  "pod.logsOf": (pod) => `${pod} 的日志`,
  "resources.title": "资源",
  "resources.hint": "Pod 模板里的预留量和上限。查看实际用量需要 metrics-server。",
  "resources.container": "容器",
  "resources.cpu": "CPU",
  "resources.memory": "内存",
  "resources.requests": "预留量",
  "resources.limits": "上限",
  "connections.title": "连接",

  "logs.noPods": "没有运行中的副本",
  "logs.noPodsBody": "日志来自运行中的副本；副本启动后，日志就会出现在这里。",
  "logs.podOf": (pod) => `副本 ${pod}`,
  "logs.replaced": (old, current) => `${old} 已被替换；改为显示 ${current}。`,
  "logs.gone": (pod) => `${pod} 已不存在；正在查找接替它的副本。`,
  "logs.container": "容器",
  "logs.previous": "上一次运行",
  "logs.timestamps": "时间戳",
  "logs.wrap": "自动换行",
  "logs.follow": "实时跟踪",
  "logs.following": "正在跟踪",
  "logs.filter": "筛选",
  "logs.filterHint": "以 - 开头可排除匹配的行。",
  "logs.error": "错误",
  "logs.warn": "警告",
  "logs.info": "信息",
  "logs.download": "下载",
  "logs.lines": "日志内容",
  "logs.empty": "这个副本还没有写出任何日志。",
  "logs.noMatch": "没有匹配的行。",
  "logs.jump": "跳到最新",
  "logs.ended": "日志流已结束",
  "logs.connecting": "连接中…",
  "logs.resume": "继续跟踪",
  "logs.refresh": "刷新",
  "logs.unavailable.no-previous-run": "没有上一次运行：容器还没有重启过。",
  "logs.unavailable.not-started": "还没有日志：容器尚未启动。",
  "logs.unavailable.removed": "这次运行的容器和日志都已从集群中删除。",
  "logs.unavailableFix.no-previous-run": "改看本次运行的日志。",
  "logs.unavailableFix.not-started": "等待容器启动；副本的事件会说明它在等什么。",
  "logs.unavailableFix.removed": "稍后再刷新。要在重启后保留日志，可以把日志发送到日志存储服务。",
  "logs.currentRun": "看本次运行",
  "logs.count": (n) => `${n} 行`,
  "logs.of": (n) => `共 ${n} 行`,
  "terminal.label": "终端",
  "terminal.noPodsBody": "终端要在运行中的副本里打开；副本启动后就能使用。",
  "terminal.pod": "副本",
  "terminal.connecting": "正在连接…",
  "terminal.open": "已连接",
  "terminal.reconnect": "重新连接",
  "terminal.exited": (code) => `已退出，退出码 ${code}`,
  "terminal.ended": "已结束",
  "terminal.noShell": "这个容器没有 shell（Distroless 镜像不含 shell）。改为打开一个临时调试容器：",
  "terminal.failed": "容器运行时结束了这个会话。",
  "terminal.expired": "会话已过期。重新连接即可继续。",
  "terminal.adminEnded": "管理员权限已结束。重新连接后将改用你自己的权限。",
  "terminal.idle": "15 分钟没有输入或输出，已关闭。",
  "terminal.lost": "连接已断开。",
  "terminal.rbac":
    "要打开终端，需要有权限在这个项目的容器里运行命令。把下面的配置交给管理员；这份配置只授予这项权限，也只限这个项目：",
  "terminal.roleLabel": "Role 和 RoleBinding",
  "terminal.debugLabel": "kubectl debug",

  "vars.title": "环境变量",
  "vars.about": "应用启动时读取的设置，例如数据库地址或 API 密钥。",
  "vars.emptyTitle": "还没有环境变量",
  "vars.emptyBody": "添加环境变量后，无需构建新镜像就能修改设置。",
  "vars.direct": "直接设置",
  "vars.fromObjects": "来自配置文件和机密",
  "vars.fromCluster": "由 Kubernetes 填入",
  "vars.reveal": (secret) => `显示 ${secret} 的值`,
  "vars.hide": (secret) => `隐藏 ${secret} 的值`,
  "vars.secretsHint": "关于 Secret 的值",
  "vars.revealHint": "Zeabur 只在你要显示时才读取值，并且只保留在本页面。",
  "vars.view": "查看方式",
  "vars.list": "列表",
  "vars.names": (service) => `指向服务 ${service}`,
  "vars.atRuntime": "运行时确定",
  "vars.allKeys": "所有键",
  "vars.keys": (n) => `${n} 个键`,

  "net.path": "路径",
  "net.publicTitle": "公开域名",
  "net.publicAbout": "互联网上的任何人都能打开这些地址。",
  "net.publicEmpty": (name) => `${name} 还没有公开域名`,
  "net.publicEmptyBody": "添加域名后，就能用浏览器打开这个服务。",
  "net.publicNeedsPrivate": "域名会把请求转发到服务的私有地址，所以要先有私有地址。",
  "net.sendsTo": "转发到",
  "net.privateTitle": "私有网络",
  "net.privateAbout": "集群里的其他服务通过这些地址连接它；互联网无法访问这些地址。",
  "net.privateEmpty": (name) => `${name} 没有私有地址`,
  "net.privateEmptyBody": "有了私有地址，其他服务和它的公开域名才能把流量发给它。",
  "net.shortForm": "同项目内的简写",
  "net.forwardsTo": (port) => `转发到容器的端口 ${port}`,
  "net.exposeHint": "要添加私有地址，请应用 Service 或运行这条命令。",
  "net.noneTitle": (name) => `${name} 无法接收连接`,
  "net.noneBody":
    "它没有私有地址，也没有公开域名。只主动向外连接的后台任务两者都不需要，仍然可以连接其他服务和互联网。",
  "net.scheduledTitle": (name) => `${name} 不接收流量`,
  "net.scheduledBody": "批处理任务运行完就结束，不会有连接进来；它仍然可以连接其他服务和互联网。",
  "reach.about": "集群允许哪些连接进出这个服务。",
  "reach.fromDomains": "公开域名",
  "net.anyHost": "任何域名",
  "net.defaultBackend": "默认后端",

  "storage.requested": "申请容量",
  "storage.title": "卷",
  "storage.about":
    "写入卷的文件，在副本重启或迁移到其他节点后仍会保留；容器里其他位置的文件，重启后就会丢失。",
  "storage.emptyTitle": (name) => `${name} 没有卷`,
  "storage.emptyBody": "应用需要长期保存文件时（例如用户上传的文件或数据库），就添加一个卷。",
  "storage.emptyJob": "要为 Job 添加卷，请编辑它的 YAML。",
  "storage.bound": "就绪",
  "storage.pending": "等待存储",
  "storage.lost": "存储丢失",
  "storage.mountPath": "挂载路径",
  "storage.accessMode": "访问模式",
  "storage.class": "存储类",
  "storage.defaultClass": "默认",
  "storage.otherMounts": "其他挂载",

  "palette.title": "命令面板",
  "palette.search": "跳转到项目、服务、副本或域名",
  "palette.recent": "最近",
  "palette.actions": "操作",
  "palette.openLogs": (name) => `打开 ${name} 的日志`,
  "palette.openTerminal": (name) => `打开 ${name} 的终端`,
  "palette.openVariables": (name) => `编辑 ${name} 的变量`,
  "palette.openChanges": (name) => `查看 ${name} 的部署记录`,
  "palette.toggleTheme": "切换主题",
  "palette.components": "组件",
  "palette.empty": "找不到匹配的项目。",
  "palette.in": (what, namespace) => `${namespace} 的 ${what}`,
  "palette.move": "移动",
  "palette.open": "打开",

  "settings.title": "设置",
  "settings.preferences": "偏好设置",
  "settings.language": "语言",
  "settings.languageHint": "来自集群的消息保持英文。",
  "settings.theme": "主题",
  "settings.about": "关于",
  "settings.user": "当前用户",
  "settings.groups": "用户组",
  "settings.kubernetesVersion": "Kubernetes",
  "settings.catalogueCommit": "Zeabur Catalog",
  "settings.components": "组件",
  "settings.filter": "筛选",
  "settings.searchComponents": "搜索组件",
  "settings.installed": "已安装",
  "settings.recommended": "推荐",
  "settings.optional": "可选",
  "settings.unsupported": "不支持",
  "settings.installCommand": "安装",
  "settings.homepage": "主页",
  "settings.opensInNewTab": "（在新标签页打开）",
  "category.traffic": "流量",
  "category.observability": "可观测性",
  "category.scaling": "扩缩容",
  "category.security": "安全",
  "category.data": "数据",
  "category.operations": "运维",
  "category.other": "其他",
  "cap.certificates": "证书",
  "cap.dns": "DNS 记录",
  "cap.event-autoscaling": "事件驱动自动扩缩容",
  "cap.ingress": "Ingress",
  "cap.intrusion-prevention": "入侵防护",
  "cap.logs": "日志存储",
  "cap.metrics": "指标",
  "cap.node-updates": "节点更新",
  "cap.object-storage": "对象存储",
  "cap.observability-operator": "监控 operator",
  "cap.postgres": "PostgreSQL 数据库",
  "cap.resource-recommendations": "资源建议",
  "cap.runtime-security": "运行时安全",
  "cap.uptime-probes": "可用性探测",
  "cap.volume-backup": "卷备份",
  "cap.vulnerability-scanning": "漏洞扫描",
  // 对象与 YAML
  "project.entry.objects": "对象",
  // 项目的 Namespace
  "project.entry.settings": "设置",
  "action.adopt": "纳入",
  "action.unadopt": "不再显示为项目",
  "action.saveDetails": "保存详细信息",
  "title.adopt": (ns) => `纳入 ${ns}`,
  "title.unadopt": (ns) => `${ns} 不再显示为项目`,
  "title.details": (ns) => `${ns} 的详细信息`,
  "title.pauseAll": (ns) => `暂停 ${ns} 的所有服务`,
  "title.restartAll": (ns) => `重启 ${ns} 的所有服务`,
  "confirm.adopt": "为这个 Namespace 加上项目标签，其中的对象都不会改变。",
  "confirm.unadopt": "这个 Namespace 不再显示为项目，其中的对象都不会删除。",
  "confirm.details": "把名称、描述和图标保存到 Namespace 上。",
  "access.title": (ns) => `无法更改 ${ns}`,
  "access.you": "<你的用户名>",
  "access.ask": (ns) => `向集群管理员申请 Namespace ${ns} 的 patch 权限。`,
  "access.alsoLabels":
    "这项权限也能修改 Namespace 的其他标签，包括 Pod 安全级别（pod-security.kubernetes.io/*）。Kubernetes 无法只限定一个标签。",
  "access.role": "用 resourceNames 限定到这个 Namespace 的 ClusterRole：",
  "access.roleLabel": "ClusterRole 与绑定",
  "access.kubectl": "或由管理员运行：",
  "access.kubectlLabel": "kubectl 命令",
  "projectSettings.description": "描述",
  "projectSettings.icon": "图标",
  "projectSettings.tooLong": (max) => `缩短到 ${max} 个字符以内。`,
  "projectSettings.readOnly": "可以查看，但无法更改。",
  "projectSettings.getAccess": "如何获得权限",
  "projectSettings.changeDetails": "更改这个项目的信息",
  "projectSettings.system": "这是 Kubernetes 留给自身组件的 Namespace，不能作为项目。",
  "projectSettings.title": "项目设置",
  "projectSettings.back": "返回",
  "projectSettings.general": "通用设置",
  "projectSettings.basic": "基本信息",
  "projectSettings.iconHint": "显示在项目名称旁。",
  "projectSettings.changeIcon": "更换图标",
  "projectSettings.name": "项目名称",
  "projectSettings.nameHint": (ns) => `代替 ${ns} 显示。Namespace 本身的名称无法更改。`,
  "projectSettings.descriptionHint": "项目的用途，显示在名称下方。",
  "projectSettings.namespaceGroup": "Kubernetes Namespace",
  "projectSettings.namespace": "Namespace",
  "projectSettings.namespaceHint": "这个项目就是这个 Namespace，名称无法更改。",
  "projectSettings.shown": "显示为项目",
  "projectSettings.shownYes": "是。Namespace 带有 Zeabur 的项目标签。",
  "projectSettings.shownNo": "否。纳入只会加上 Zeabur 的项目标签，其他都不变。",
  "projectSettings.batch": "批量操作",
  "projectSettings.pauseAll": "暂停所有服务",
  "projectSettings.pauseAllHint": "把每个运行中的服务缩放到 0 个副本。其他工具管理的服务不会变动。",
  "projectSettings.pause": "暂停",
  "projectSettings.restartAll": "重启所有服务",
  "projectSettings.restartAllHint": "分批替换副本，涵盖所有能重启的服务。",
  "projectSettings.danger": "危险区域",
  "projectSettings.remove": "从 Zeabur 移除",
  "projectSettings.removeHint": "只会移除 Zeabur 的项目标签，Namespace 和其中的一切都会保留。",
  "projectSettings.removeAction": "移除",
  "icon.folder": "文件夹",
  "icon.globe": "地球",
  "icon.storefront": "商店",
  "icon.rocket": "火箭",
  "icon.cube": "方块",
  "icon.database": "数据库",
  "icon.cloud": "云",
  "icon.code": "代码",
  "icon.terminal": "终端",
  "icon.chart": "图表",
  "icon.chat": "聊天",
  "icon.book": "书",
  "icon.flask": "烧瓶",
  "icon.game": "游戏",
  "icon.heart": "爱心",
  "icon.briefcase": "公文包",
  "objects.filter": "筛选对象",
  "objects.generated": "自动生成",
  "objects.count": (n: number) => `${n} 个对象`,
  "objects.none": "这个项目没有对象。",
  "objects.noMatch": (query) => `没有符合“${query}”的对象。`,
  "objects.owner": (kind: string) => `由 ${kind} 创建`,
  "objects.empty": "空的",
  "objects.scan": "扫描全部类型",
  "objects.scanning": "正在扫描…",
  "objects.scanned": "已读取全部类型",
  "objects.updated": "更新于",
  "objects.refresh": "刷新",
  "objects.load": (kind: string) => `读取 ${kind}`,
  "objects.unread": "未读取",
  "objects.denied": "没有权限",
  "objects.pin": (kind: string) => `固定 ${kind}`,
  "objects.unpin": (kind: string) => `取消固定 ${kind}`,
  "tab.yaml": "YAML",
  "yaml.hidden": "Secret 的值已隐藏。",
  "yaml.label": (name: string) => `${name} 的 YAML`,

  // 变更计划
  "plan.planning": "正在计算…",
  "plan.apply": "应用",
  "plan.applying": "正在应用…",
  "plan.cancel": "取消",
  "plan.replan": "重新预览",
  "plan.details": "技术细节",
  "plan.expiresIn": (time: string) => `${time} 后失效`,
  "plan.expired": "已失效",
  "plan.warnings": "应用前须知",
  "plan.owners": "字段所有者",
  "plan.diff": "YAML 差异",
  "plan.create": "新建",
  "plan.delete": "删除",
  "plan.typeName": (name: string) => `输入 ${name} 以确认`,
  "plan.added": "新增",
  "plan.removed": "移除",
  "warn.restartNeeded": (names: string) => `重启后才会生效：${names}`,
  "warn.restartAdded": (names: string) => `也会重启 ${names}`,
  "warn.tlsHostChanged": (host: string, route: string) =>
    `${route} 的 HTTPS 配置仍是旧域名；${host} 需要新的证书`,
  "warn.gatewayChosen": (gateway: string, of: number) =>
    `连接到 Gateway ${gateway}（共找到 ${of} 个）`,
  "warn.gatewaysUnreadable": "无法读取 Gateway：Zeabur 改用 Ingress",
  "warn.secretValuesUnknown": (name: string) => `无法读取 ${name} 的值：差异只列出键名`,
  "warn.configNotRolledBack": (workload: string) =>
    `${workload} 读取 ConfigMap 或 Secret：回滚不会改动它们的内容`,
  "warn.storageClassUnknown": (claim: string) =>
    `无法读取 ${claim} 的 StorageClass：能否扩容由集群决定`,
  "warn.readWriteOnceShared": (workload: string) =>
    `${workload} 有多个副本：ReadWriteOnce 卷只能挂在一个节点上`,
  "warn.readWriteOncePodShared": (workload: string) =>
    `${workload} 有多个副本：ReadWriteOncePod 卷一次只能给一个副本使用`,
  "warn.deletesDependents": (owner: string, names: string) =>
    `也会删除 ${owner} 拥有的对象：${names}`,
  "warn.podReplaced": (pod: string, owner: string) => `${owner} 会启动新的副本替代 ${pod}`,
  "warn.claimDataDeleted": (claim: string, volume: string) =>
    `集群会删除 ${claim} 的数据：卷 ${volume} 的回收策略是 Delete`,
  "warn.claimDataRetained": (claim: string, volume: string) =>
    `卷 ${volume} 会保留 ${claim} 的数据（回收策略 Retain），直到管理员移除`,
  "warn.claimDataUnknown": (claim: string) => `${claim} 的数据可能丢失：无法读取其卷的回收策略`,
  "warn.stillUsed": (name: string, names: string) =>
    `${names} 仍在读取 ${name}：少了它，新的副本无法启动`,
  "warn.claimStillMounted": (name: string, names: string) =>
    `${names} 仍挂载 ${name}：等到没有副本使用后，Kubernetes 才会移除它`,
  "warn.ingressOpened": (names: string) => `不再隔离：${names} 会接受任何来源的流量`,
  "warn.egressOpened": (names: string) => `不再限制：${names} 可以把流量发送到任何地方`,
  "warn.connectionsOpened": (pairs: string) => `恢复允许：${pairs}`,
  // 确认
  "confirm.restartSingle": (name) => `${name} 只有一个副本，重启时可能中断几秒。`,
  "confirm.restartNone": (name) => `${name} 当前没有就绪的副本，重启会替换全部副本。`,
  "confirm.rolling": (name, pods) => `${name} 的 ${pods} 个副本会逐个替换，服务不会中断。`,
  "confirm.rollingSingle": (name) => `${name} 只有一个副本，替换时可能中断几秒。`,
  "confirm.rollingNone": (name) => `${name} 的副本会全部替换。`,
  "confirm.envSet": (names, service) => `在 ${service} 设置 ${names}。`,
  "confirm.envRemove": (names, service) => `从 ${service} 移除 ${names}。`,
  "confirm.envBoth": (set, removed, service) => `在 ${service} 设置 ${set}，并移除 ${removed}。`,
  "confirm.dataSet": (keys, kind, name) => `在 ${kind} ${name} 设置 ${keys}。`,
  "confirm.dataRemove": (keys, kind, name) =>
    `从 ${kind} ${name} 删除 ${keys}。删除的值无法在这里恢复。`,
  "confirm.dataBoth": (set, removed, kind, name) =>
    `在 ${kind} ${name} 设置 ${set}，并删除 ${removed}。删除的值无法在这里恢复。`,
  "confirm.keepsOld": (names) => `${names} 在重启前仍使用旧值。`,
  "confirm.readersKeepOld": (names, object) => `${names} 读取 ${object}，在重启前仍使用旧值。`,
  "confirm.alsoReads": (names, object) => `也会影响 ${names}（读取 ${object}）。`,
  "confirm.alsoAffects": (names) => `也会影响 ${names}。`,
  "confirm.addDomain": (host, service) => `${host} 会指向 ${service}。`,
  "confirm.renameDomain": (from) => `指向 ${from} 的链接会失效。`,
  "confirm.removeDomain": (host) => `访问 ${host} 的请求不会再进入这个项目。`,
  "confirm.rollback": (revision) => `修订版本 ${revision} 之后的模板变更都会撤销。`,
  "confirm.notRolledBack": "回滚不会改动 ConfigMap 和 Secret 的内容。",
  "confirm.noShrink": "卷扩容后就无法缩小。",
  "confirm.addVolume": (size, service) => `${service} 会多一个 ${size} 的卷。`,
  "confirm.moveMount": (service, from, to) => `把 ${service} 的挂载从 ${from} 移到 ${to}。`,
  "confirm.readOnly": (path, service) => `把 ${service} 的 ${path} 设为只读。`,
  "confirm.writable": (path, service) => `把 ${service} 的 ${path} 设为可写。`,
  "confirm.mount": (service) => `修改 ${service} 的挂载。`,
  "confirm.probeSet.readiness": (check, period) =>
    `${check}成功之前，副本不会收到流量（每 ${period} 秒检查一次）。`,
  "confirm.probeSet.liveness": (check, period, failures) =>
    `${check}连续失败 ${failures} 次，集群就会重启容器（每 ${period} 秒检查一次）。`,
  "confirm.probeSet.startup": (check, period, failures) =>
    `${check}成功之前，其他检查先暂停；失败 ${failures} 次后，集群会重启容器（每 ${period} 秒检查一次）。`,
  "confirm.probeRemoved.readiness": "移除后，副本一启动就会收到流量。",
  "confirm.probeRemoved.liveness": "移除后，集群不会重启卡住的容器。",
  "confirm.probeRemoved.startup": "移除后，其他检查会在容器一启动就开始。",
  "confirm.check.http": (port, path) => `对 ${port} 端口的 ${path} 发 GET 请求`,
  "confirm.check.tcp": (port) => `连接 ${port} 端口`,
  "confirm.check.grpc": (port) => `${port} 端口的 gRPC 健康检查`,
  "confirm.check.exec": (command) => `运行 ${command} `,
  "confirm.scale": (name, from, to) => `把 ${name} 从 ${from} 个副本调整为 ${to} 个。`,
  "confirm.scaleTo": (name, to) => `把 ${name} 调整为 ${to} 个副本。`,
  "confirm.scaleZero": (name) => `${name} 会停止；再次调高副本数之前，不会有副本运行。`,
  "confirm.resources": (pairs) =>
    `${pairs.map(([label, value]) => `${label} ${value}`).join("，")}。`,
  "confirm.yamlAsIs": "Zeabur 会原样写入 YAML，你改动的每个字段都会立即生效。",
  "confirm.create": (kind, name) => `创建 ${kind} ${name}。`,
  "confirm.isolates": (name) => `${name} 之后只收发这条和其他策略允许的流量，其余流量集群会拦截。`,
  "confirm.delete": (name: string) => `删除 ${name}。删除后无法恢复。`,
  "action.restart": "重启",
  "action.saveVariables": "保存环境变量",
  "action.saveValues": "保存值",
  "action.saveAndRestart": "保存并重启",
  "action.addDomain": "添加域名",
  "action.renameDomain": "更改域名",
  "action.removeDomain": "移除域名",
  "action.rollback": (revision) => `回滚到修订版本 ${revision}`,
  "action.grow": (size) => `扩容到 ${size}`,
  "action.addVolume": "添加卷",
  "action.saveMount": "保存挂载",
  "action.saveHealthChecks": "保存健康检查",
  "action.scale": (replicas) => `调整为 ${replicas} 个副本`,
  "action.saveResources": "保存资源",
  "action.save": "保存",
  "action.saveYaml": "保存 YAML",
  "action.isolate": "隔离",
  "action.allow": "允许",
  "action.create": "创建",
  "action.delete": "删除",
  // 通知
  "toast.region": "通知",
  "toast.applied": "已应用",
  "toast.view": "查看",
  "toast.undo": "撤销",
  "toast.undone": "已撤销",
  "toast.managed": (tool) => `由 ${tool} 管理`,
  "toast.managedHelm": "在 chart 的 values 里修改，再升级 release。",
  "toast.managedGit": (tool) => `在 ${tool} 同步的 Git 仓库里修改。`,
  "title.env": (service: string) => `${service} 的环境变量`,
  "title.data": (kind: string, name: string) => `${kind} ${name}`,
  "title.restart": (service: string) => `重启 ${service}`,
  "title.addDomain": (host: string) => `添加 ${host}`,
  "title.renameDomain": (from: string, to: string) => `${from} → ${to}`,
  "title.removeDomain": (host: string) => `移除 ${host}`,
  "yaml.edit": "编辑",
  "yaml.editSecret": "保持 (hidden) 的值会保留原值。",
  "yaml.notEditable": "这类对象由 Kubernetes 自行管理，改为编辑拥有它的对象。",
  "title.editYaml": (name: string) => `编辑 ${name}`,
  "title.delete": (kind: string, name: string) => `删除 ${kind} ${name}`,
  "yaml.delete": "删除",
  "yaml.notDeletable": "这类对象由 Kubernetes 自行创建，改为删除拥有它的对象。",
  "title.rollback": (service: string, revision: number) => `将 ${service} 回滚到 #${revision}`,
  "title.compute": (service: string) => `${service} 的计算资源`,
  "title.probe": (service: string) => `${service} 的健康检查`,
  "title.expand": (claim: string, size: string) => `将 ${claim} 扩容到 ${size}`,
  "title.mount": (path: string) => `修改挂载 ${path}`,
  "title.addVolume": (path: string) => `在 ${path} 添加卷`,
  "releases.none": "每次更换镜像或修改设置，这里就会多一条部署记录。",
  "releases.noneTask": "Job 和 CronJob 不保留发布记录。",
  "releases.current": "运行中",
  "releases.compare": "比较",
  "releases.rollback": "回滚",
  "releases.diff": (from: number, to: number) => `#${from} → #${to}`,
  "releases.same": "两个模板相同。",
  "form.compute": "计算资源",
  "form.health": "健康检查",
  "form.jobs": "Job 只运行一次，模板要用 YAML 修改。",
  "form.replicas": "副本数",
  "form.cpuRequest": "CPU 预留量",
  "form.notSet": "未设置",
  "form.cpuUnset": "未设置时不预留。250m 是四分之一核。",
  "form.memoryUnset": "未设置时不预留。例如 256Mi。",
  "form.noLimit": "未设置时不设上限",
  "form.memoryRequest": "内存预留量",
  "form.cpuLimit": "CPU 上限",
  "form.memoryLimit": "内存上限",
  "form.computeHint":
    "节点会按预留量为每个副本留出资源。副本使用的内存超过上限时，容器会重启；CPU 超过上限时会降速。",
  "form.largestNode": (cores: string, memory: string) => `最大的节点：${cores} 核、${memory}`,
  "form.badQuantity": "输入数量，例如 250m、1 或 512Mi。",
  "form.badReplicas": "输入 0 到 1000 的整数。",
  "form.badNumber": "输入大于 0 的整数。",
  "form.badPath": "输入绝对路径，例如 /data。",
  "form.badName": "只能用小写字母、数字和 -。",
  "form.probe.readiness": "就绪检查",
  "form.probe.liveness": "存活检查",
  "form.probe.startup": "启动检查",
  "form.probeEffect.readiness": "只有通过的副本会收到流量。",
  "form.probeEffect.liveness": "容器持续失败时，集群会重启它。",
  "form.probeEffect.startup": "应用启动前，先暂缓其他检查。",
  "form.probeCheck": "检查方式",
  "form.probeNone": "不检查",
  "form.probeExec": "命令",
  "form.probePath": "路径",
  "form.probePort": "端口",
  "form.probeCommand": "命令",
  "form.probeCommandMissing": "输入要运行的命令。",
  "form.probePeriod": "间隔（秒）",
  "form.probeFailures": "失败次数",
  "form.probePreset": (port: string) => `HTTP，端口 ${port}`,
  "form.size": "大小",
  "form.sizeHint": "只能扩容，不能缩小。",
  "form.expand": "扩容",
  "form.readOnly": "只读",
  "form.addVolume": "添加卷",
  "form.volumeName": "名称",
  "form.statefulVolumes": "StatefulSet 的卷在创建时就固定了。",
  "form.advanced": "高级",
  "form.claimName": "PVC 名称",
  "form.claimNameHint": "会创建新的 PVC；集群会拒绝已在使用的名称。",
  "form.badClaimName": "只能用小写字母、数字、- 和 .，最多 253 个字符。",
  "form.accessMode": "访问模式",
  "form.accessModeHint": "多节点模式需要支持它的存储。",
  "ops.managedEdit": (tool: string) => `由 ${tool} 管理，需要在它的来源修改`,

  // 编辑
  "edit.add": "添加",
  "edit.review": "预览变更",
  "edit.cancel": "取消",
  "edit.edit": (name: string) => `编辑 ${name}`,
  "edit.remove": (name: string) => `移除 ${name}`,
  "edit.undo": (name: string) => `保留 ${name}`,
  "vars.add": "添加环境变量",
  "vars.name": "名称",
  "vars.value": "值",
  "vars.container": "容器",
  "vars.editIn": (kind: string, name: string) => `在 ${kind} ${name} 编辑`,
  "data.addKey": "添加键",
  "data.key": "键",
  "data.restartAfter": "应用后重启读取它的服务",
  "data.readers": "读取者",
  "data.reveal": "显示值",
  "data.binary": "二进制",
  "data.noChanges": "没有变更。",
  "net.addDomain": "添加域名",
  "net.domain": "域名",
  "diagnosis.title": "出了什么问题",
  "diagnosis.evidence": "依据",
  "diagnosis.replica": "副本",
  "diagnosis.exitCode": "退出码",
  "diagnosis.action.logs": "打开日志",
  "diagnosis.action.memory": "调整内存",
  "diagnosis.action.healthChecks": "健康检查",
  "diagnosis.action.networking": "网络",
  "diagnosis.action.events": "打开事件",
  "diagnosis.action.replicas": "调整副本数",
  "diagnosis.action.reservation": "调整预留量",
  "diagnosis.action.variables": "变量",
  "diagnosis.action.storage": "存储",
  "gap.title": "缺少的设置",
  "gap.warning": "警告",
  "gap.advice": "建议",
  "gap.fix": "修正",
  "gap.install": "安装",
  "gap.snippet": "要加入的 YAML",
  "fix.resources": (container: string) => `为 ${container} 设置 CPU 和内存`,
  "fix.readiness": (container: string) => `为 ${container} 添加就绪检查`,
  "fix.addVolume": (container, path) => `把 ${container} 的 ${path} 存到磁盘`,
  "fix.isolateProject": (project: string) => `隔离 ${project} 的服务`,
  "gap.unreadable": "部分检查需要你没有的权限。",
  "install.title": "安装",
  "install.note": "Zeabur 不会运行这些内容。自己运行，或提交到 GitOps 仓库。",
  "install.profile": "安装方式",
  "install.singleNode": "单节点",
  "install.multiNode": "多节点",
  "install.forCluster": "适合这个集群",
  "install.needs": "需要先安装",
  "install.security": "权限",
  "install.clusterWide": "集群范围的 RBAC",
  "install.privileged": "特权模式",
  "install.connectsTo": "会连接到",
  "install.credentials": "凭证",
  "install.risks": "风险",
  "install.inputs": "需要填入的值",
  "install.generated": "自动生成",
  "install.required": "必填",
  "install.alsoAdds": "也会为这些组件添加值",
  "install.k3sPackaged": (addon: string) =>
    `在 k3s 上，先禁用内置的 ${addon}：在每台 server 的 /etc/rancher/k3s/config.yaml 中把它加入 disable，然后重启 k3s。`,
  "install.k3sPackagedTraefik":
    "这会删除 Traefik 的 CRD 以及所有 IngressRoute、Middleware 对象；这个组件开始运行前，Ingress 流量会中断。",
  "install.k3sPackagedMetrics": "这个组件开始运行前，metrics API 不可用。",
  "install.k3sPackagedOther": "这个组件开始运行前，内置版本的工作负载会停止。",
  "reach.title": "连接规则",
  "reach.addPolicy": "添加规则",
  "reach.isolate": "隔离这个服务",
  "reach.allowIngress": "允许来自公开域名的流量",
  "reach.allowFrom": (name) => `允许 ${name} 连入`,
  "reach.planIsolate": (name) => `隔离 ${name}`,
  "reach.planAllowIngress": (name) => `允许公开域名连接 ${name}`,
  "reach.planAllowFrom": (from, to) => `允许 ${from} 连到 ${to}`,
  "reach.policies": "规则",
  "reach.noPolicies": "这个项目没有连接规则，所以允许所有连接。",
  "reach.noneSelect": "没有规则应用到这个服务，所以允许所有连接。",
  "reach.in": "入站",
  "reach.out": "出站",
  "reach.otherNamespaces": "其他 Namespace",
  "reach.dns": "域名解析",
  "reach.outside": "集群外",
  "reach.cni": "规则是否真正生效，取决于集群的网络插件（CNI）。",
  "reach.allowed": "允许",
  "reach.limited": "部分",
  "reach.blocked": "拦截",
  "reach.edgeBlocked": "连接规则不允许",
  "reach.edgeLimited": "连接规则只允许一部分",
  "backups.title": "备份",
  "backups.namespace": "Namespace 内的所有对象",
  "backups.unscheduled": "没有备份计划",
  "backups.none": "没有备份",
  "backups.noRun": "还没备份过",
  "backups.noProvider": "要备份卷，请先安装备份组件。",
  "backups.seeComponents": "打开组件",
  "backups.refused": (module) => `没有权限读取 ${module} 的备份。`,
  "backups.details": "备份计划与记录",
  "backups.paused": "已暂停",
  "backups.succeeded": "成功",
  "backups.partiallyFailed": "部分失败",
  "backups.failed": "失败",
  "backups.running": "运行中",
  "backups.unknown": "未知",
  "backups.veleroVolumesNeedSnapshots": "卷数据需配合快照",
  "backups.scheduledOf": (scheduled, total) => `${scheduled}/${total} 有备份计划`,

  // 用量与历史
  "usage.title": "当前用量",
  "usage.history": "历史",
  "usage.live": "实时",
  "usage.liveHint":
    "数值为近似值，来自 metrics-server，大约每 15 秒测量一次。内存是 working set，即 kubelet 据以判断的数字。刻度是每个副本的预留量（request）；条的末端是上限（limit），没有上限时画成斜线。",
  "usage.cores": (n) => `${n} 核`,
  "usage.coresUnit": "核",
  "usage.usedWord": (v) => `已用 ${v}`,
  "usage.reservedWord": (v) => `预留 ${v}`,
  "usage.maxWord": (v) => `上限 ${v}`,
  "usage.noMax": "没有上限",
  "usage.noReserved": "没有预留量",
  "usage.acrossPods": (n) => `（${n} 个副本合计）`,
  "usage.ofMax": (pct) => `上限的 ${pct}`,
  "usage.sampledOf": (a, b) => `已测到 ${a}/${b} 个副本`,
  "usage.perPodTemplate": (n, cpu, memory) =>
    `每个副本（×${n}）：CPU ${cpu}，内存 ${memory}（预留量 / 上限）`,
  "usage.replica": "副本",
  "usage.notes": "备注",
  "usage.nearLimitOne": (pod) => `${pod} 快用到内存上限，超过就会重启。`,
  "usage.nearLimitMany": (n) => `${n} 个副本快用到内存上限，超过就会重启。`,
  "usage.needsMetricsServer": "实时用量需要 metrics-server。",
  "usage.install": "安装",
  "usage.whatLive": "查看实时用量",
  "usage.unavailable": "metrics-server 没有响应，因此只列出预留量和上限。",
  "usage.note.oom": "内存不足",
  "usage.note.oomHint": "上次以 OOMKilled 结束：用量超过内存 limit，容器因此重启。",
  "usage.note.nearLimit": "接近内存上限",
  "usage.note.nearLimitHint": (pct) =>
    `已用内存 limit 的 ${pct}。超过 limit 时，容器会停止并重启（OOMKilled）。`,
  "usage.note.cpuAtLimit": "CPU 已达上限",
  "usage.note.cpuAtLimitHint": "受 CPU limit 限制（throttling）：会变慢，但不会重启。",
  "usage.note.overRequest": "超过预留量",
  "usage.note.overRequestHint":
    "用的内存比 request 多。机器内存不足时，kubelet 会先驱逐超过 request 的副本。",
  "usage.note.noSample": "暂无数据",
  "usage.note.noSampleHint": "metrics-server 只测量运行中的容器；新副本大约 30 秒内出现。",
  "usage.split": "显示",
  "usage.total": "合计",
  "usage.perReplica": "每个副本",
  "usage.range": "范围",
  "usage.range.1h": "1 小时",
  "usage.range.6h": "6 小时",
  "usage.range.24h": "24 小时",
  "usage.range.7d": "7 天",
  "usage.rangeLong.1h": "1 小时",
  "usage.rangeLong.6h": "6 小时",
  "usage.rangeLong.24h": "24 小时",
  "usage.rangeLong.7d": "7 天",
  "usage.reservedLine": "预留量",
  "usage.maxLine": "上限",
  "usage.chartLabel": (metric, range) => `${metric}，最近 ${range}`,
  "usage.noSampleShort": "没有数据",
  "usage.replacedPods": (n) => `${n} 个已替换的副本（淡色）`,
  "usage.markerOom": "内存不足",
  "usage.markerRestart": "重启",
  "usage.restartsUnknown": "重启次数未知",
  "usage.restartsUnknownHint": "Metrics 存储服务没有重启次数：它没有收集 kube-state-metrics。",
  "usage.needsStore": "历史数据需要 metrics 存储服务。",
  "usage.installVm": "安装 VictoriaMetrics",
  "usage.chooseStore": "改用其他存储服务",
  "usage.needsStoreHint": (service) =>
    `Zeabur 查找的是 ${service}。兼容 Prometheus 的存储服务都可以用，例如 Prometheus 或 VictoriaMetrics；在设置里指定它的 Service。`,
  "usage.whatHistory": "查看用量历史",
  "usage.proxyHint":
    "Zeabur 以你的身份通过 Kubernetes API 读取 metrics 存储服务。能这样读取的人可以看到所有项目的 metrics，所以通常只有管理员有这项权限。",
  "usage.storeUnavailable": "Metrics 存储服务没有响应。",
  "usage.noSeries": (name, range) => `最近 ${range} 没有 ${name} 的数据。`,
  "usage.noSeriesHint":
    "存储服务有响应，但没有这些副本的数据：可能没有收集这台机器的容器 metrics，或没有保留这么久。",
  "usage.source": "来源",
  "usage.stepSeconds": (n) => `每 ${n} 秒一点`,
  "usage.stepMinutes": (n) => `每 ${n} 分钟一点`,
  "usage.tenancy": "谁看得到",
  "usage.tenancyHint":
    "Zeabur 只查询这个项目的副本。但存储服务本身不分项目：能打开它的人可以读到所有项目的 metrics。",
  "usage.loadingChart": "正在加载图表…",

  // 设置：metrics 存储服务
  "settings.metricsStore": "用量历史",
  "settings.store": "历史数据来源",
  "settings.storeHint":
    "保存用量历史的 Service，格式为 namespace/name:port。任何提供 Prometheus HTTP API 的存储服务都可以。Zeabur 通过 Kubernetes API，以每个人自己的权限读取。",
  "settings.storeDefault": "默认",
  "settings.storeChosen": "在 Zeabur 中设置",
  "settings.storeState": "状态",
  "settings.storeState.available": "有响应",
  "settings.storeState.not-configured": "找不到",
  "settings.storeState.unreadable": "没有权限",
  "settings.storeState.unavailable": "没有响应",
  "settings.storeSave": "保存",
  "settings.storeReset": "改回默认",
  "settings.storeSaved": "已保存",
  "settings.storeWho": "通常只有集群管理员能修改。",
  "settings.storeWhereLabel": "设置存放位置",
  "settings.storeWhere": (namespace) =>
    `保存在 Namespace ${namespace} 的 ConfigMap zeabur-settings。能修改这个 ConfigMap 的人，就能修改这项设置。`,
  "settings.storeDenied": "你不能修改这项设置。",
  "settings.storeChange": "更改历史数据来源",
  "settings.storeLocal":
    "Zeabur 在这台电脑上运行，不保存设置。要改用其他存储服务，用 --metrics-service namespace/name:port 启动。",
  "settings.storeTenancy":
    "能读取存储服务的人，就能看到所有项目的指标。只开放给可以查看整个集群的人。",
  "sentence.problem.forbidden.fix.askForRule": (p) =>
    `向集群管理员申请 ${p.rule} 的权限，例如创建授予该权限的 ClusterRole，以及绑定到你的 ClusterRoleBinding。`,
  "sentence.problem.forbidden.fix.askOrRetry": () => "联系管理员，或以管理员身份重试。",
  "sentence.problem.forbidden.fix.sendGrant": () =>
    "把下面的配置交给管理员。它只授予这项权限，而且只在这个项目有效。",
  "sentence.problem.planExpired.fix.again": (p) =>
    `计划只在 ${p.minutes} 分钟内有效。重新生成计划。`,
  "sentence.problem.badRequest.fix.planToken": () =>
    "重新生成计划。计划只能在生成它的服务器上，用原来的令牌原样应用。",
  "sentence.problem.conflict.fix.review": () => "重新预览，从它当前的状态开始。",
  "sentence.problem.conflict.fix.waitForContainer": () =>
    "等它开始运行；它的日志和事件会说明为什么还没运行。",
  "sentence.problem.unavailable.fix.tryAgainSoon": () => "稍后再试一次。",
  "sentence.problem.logUnavailable.fix.noPreviousRun": () => "改看这一次运行的日志。",
  "sentence.problem.logUnavailable.fix.notStarted": () =>
    "等容器启动；副本的事件会说明它在等什么。",
  "sentence.problem.logUnavailable.fix.removed": () =>
    "稍后再刷新。如果还是无法读取，改看这一次运行的日志。要在重启后保留日志，把日志发送到日志存储服务。",
  "sentence.problem.quotaExceeded.fix.scale": (p) =>
    p.object
      ? `减少 ${p.object} 的副本数、调低每个副本的预留量，或请管理员调高上限 ${p.quota}。`
      : `减少请求的量、释放项目已占用的资源，或请管理员调高上限 ${p.quota}。`,
  "sentence.problem.policyDenied.fix.change": () => "修改它指出的地方，或咨询负责这项策略的人。",
  "sentence.diagnosis.crashLoop.why.backOff": (p) =>
    `它已重启 ${p.restarts} 次；集群每次都会再启动它，两次之间等得越来越久，默认最多等 5 分钟。`,
  "sentence.diagnosis.crashLoop.why.exitsZero": (p) =>
    `服务应该持续运行，所以程序一结束，即使是正常结束，集群也会再启动它（目前 ${p.restarts} 次）。`,
  "sentence.diagnosis.runFailed.why.exitCode": (p) =>
    `程序结束时的退出码不是 0 就算运行失败；这次是 ${p.exitCode}。`,
  "sentence.diagnosis.runFailed.why.gaveUp": (p) =>
    p.backoffLimit
      ? p.backoffLimit === "0"
        ? `这项任务失败后不会重试；这次的退出码是 ${p.exitCode}。`
        : `这项任务已放弃：运行失败时会重试 ${p.backoffLimit} 次，最后一次的退出码也是 ${p.exitCode}。`
      : `这项任务重试后已放弃；最后一次的退出码是 ${p.exitCode}。`,
  "sentence.diagnosis.outOfMemory.why.limit": (p) =>
    `每个副本最多只能用 ${p.limit} 内存，用到上限时系统会直接终止它，程序来不及处理。`,
  "sentence.diagnosis.outOfMemory.why.noLimit": () =>
    "没有内存上限时，它可以一直用到机器的内存不足，这时系统会终止用得最多的程序。",
  "sentence.diagnosis.imagePull.why": () =>
    "集群要先下载镜像才能启动容器；下载失败时会一再重试，每次等得更久。",
  "sentence.diagnosis.probeFailure.why.liveness": (p) => {
    const t = probeTiming(p)
    return t
      ? timingZhCN(
          t,
          `集群每 ${secondsZhCN(t.period)}检查一次，连续失败 ${t.threshold} 次（约 ${secondsZhCN(t.total)}）就会重启容器。`,
        )
      : "存活检查连续失败几次后，集群就会重启容器。"
  },
  "sentence.diagnosis.probeFailure.why.readiness": (p) => {
    const t = probeTiming(p)
    return t
      ? timingZhCN(
          t,
          `集群每 ${secondsZhCN(t.period)}检查一次；连续失败 ${t.threshold} 次（约 ${secondsZhCN(t.total)}）后，容器就不会从私有地址收到流量，直到再次通过。`,
        )
      : "就绪检查失败期间，容器不会从私有地址收到流量。"
  },
  "sentence.diagnosis.probeFailure.why.startup": (p) => {
    const t = probeTiming(p)
    return t
      ? timingZhCN(
          t,
          `容器有 ${t.threshold} × ${secondsZhCN(t.period)}（${secondsZhCN(t.total)}）可以通过启动检查；超过时集群就会重启它。`,
        )
      : "其他检查会等启动检查通过；如果没有在时限内通过，集群就会重启容器。"
  },
  "sentence.diagnosis.configMissing.cause.object": (p) =>
    `${p.container} 无法启动：${objectZhCN(p)} 不存在。`,
  "sentence.diagnosis.configMissing.cause.key": (p) =>
    `${p.container} 无法启动：${objectZhCN(p)} 里没有 ${p.key}。`,
  "sentence.diagnosis.configMissing.cause.unknown": (p) =>
    `${p.container} 无法启动：缺少它读取的机密或配置文件。`,
  "sentence.diagnosis.configMissing.cause.volume": (p) =>
    `${mountedZhCN(p)} 无法启动：卷 ${p.volume} 来自 ${objectZhCN(p)}，但它不存在。`,
  "sentence.diagnosis.configMissing.why": () =>
    "容器读取的每个机密、配置文件和其中的键都存在之后，集群才会启动它。",
  "sentence.diagnosis.configMissing.why.volume": () =>
    "集群会先挂载所有卷，再启动副本的容器；卷如果来自不存在的机密或配置文件，就无法挂载，容器会一直等待。",
  "sentence.diagnosis.configMissing.fix.object": (p) =>
    `在项目里加上 ${objectZhCN(p)}，或不再读取它。`,
  "sentence.diagnosis.configMissing.fix.key": (p) =>
    `在 ${objectZhCN(p)} 加上 ${p.key}，或不再读取它。`,
  "sentence.diagnosis.configMissing.fix.unknown": () =>
    "副本的事件会说缺少什么；把它加上，或不再读取它。",
  "sentence.diagnosis.configMissing.fix.volume": (p) =>
    `在项目里加上 ${objectZhCN(p)}，或移除卷 ${p.volume}。`,
  "sentence.diagnosis.unschedulable.cause.cpu": (p) =>
    `${p.service} 正在等待可用资源：没有机器有足够的 CPU。`,
  "sentence.diagnosis.unschedulable.cause.memory": (p) =>
    `${p.service} 正在等待可用资源：没有机器有足够的内存。`,
  "sentence.diagnosis.unschedulable.cause.placement": (p) =>
    `${p.service} 正在等待可以运行它的机器。`,
  "sentence.diagnosis.unschedulable.cause.other": (p) => `${p.service} 正在等待可以运行的机器。`,
  "sentence.diagnosis.unschedulable.why": (p) =>
    `每个副本要放到容得下它预留量的机器上；集群的说明：${p.message}`,
  "sentence.diagnosis.unschedulable.fix.cpu": () => "减少预留的 CPU、减少副本数，或加一台机器。",
  "sentence.diagnosis.unschedulable.fix.memory": () => "减少预留的内存、减少副本数，或加一台机器。",
  "sentence.diagnosis.unschedulable.fix.placement": () =>
    "检查它可以在哪里运行（node selector、affinity、tolerations）。",
  "sentence.diagnosis.unschedulable.fix.other": () => "副本的事件里有调度器完整的响应。",
  "sentence.diagnosis.quotaBlocked.cause": (p) =>
    `${p.service} 无法再启动更多副本：已达到项目用量上限 ${p.quota}。`,
  "sentence.diagnosis.quotaBlocked.why": (p) =>
    `项目用量上限 ${p.quota} 限制所有服务加起来能预留的量；超过时集群会拒绝新的副本。`,
  "sentence.diagnosis.quotaBlocked.fix": () =>
    "减少副本数、调低每个副本的预留量，或请管理员调高上限。",
  "sentence.diagnosis.unreachable.cause.noService": (p) =>
    `集群里的其他服务还连不到 ${p.service}：它没有私有地址（Service）。`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p) =>
    `${p.endpoint} 把流量送到端口 ${p.targetPort}，但 ${p.service} 的容器没有声明这个端口${p.ports ? `（声明的是 ${p.ports}）` : ""}。`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p) =>
    `${p.endpoint ?? p.address} 没有地方可以送流量：${p.service} 没有任何就绪的副本。`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p) =>
    `发往 ${p.route} 域名的请求到不了 ${p.service}：流量入口报告 ${p.condition} ${p.reason}。`,
  "sentence.diagnosis.unreachable.why.noService": (p) =>
    p.from
      ? `${p.from} 用名称 ${p.service} 发送流量给它，但要有同名的私有地址（Service）才收得到。`
      : `路由用名称 ${p.endpoint ?? p.service} 发送流量，但要有同名的私有地址（Service）才收得到。`,
  "sentence.diagnosis.unreachable.why.targetPort": () =>
    "私有地址会转发到它指定的端口；用名称指定时，必须是容器声明的端口。",
  "sentence.diagnosis.unreachable.why.noReadyEndpoints": () =>
    "私有地址只会把流量送给通过就绪检查的副本。",
  "sentence.diagnosis.unreachable.why.routeNotAccepted": () =>
    "域名的流量入口必须接受这条路由，并找到它指定的地址，请求才会通过。",
  "sentence.diagnosis.unreachable.fix.noService": () =>
    "添加私有地址（Service），让其他服务或域名可以把流量发给它。",
  "sentence.diagnosis.unreachable.fix.targetPort": () => "让私有地址指向容器声明的端口。",
  "sentence.diagnosis.unreachable.fix.noReadyEndpoints": () =>
    "找出为什么没有副本就绪：它的其他问题、就绪检查、日志。",
  "sentence.diagnosis.unreachable.fix.routeNotAccepted": () =>
    "检查路由的流量入口和它指向的地址；路由的状态会说是哪一个。",
  "sentence.diagnosis.unexplained.cause.waiting": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 正在等待（${p.reason}）。`
      : `${p.pod} 没有在运行：${p.container} 正在等待（${p.reason}）。`,
  "sentence.diagnosis.unexplained.cause.terminated": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 已停止（${p.reason}）。`
      : `${p.pod} 没有在运行：${p.container} 已停止（${p.reason}）。`,
  "sentence.diagnosis.unexplained.cause.pending": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 还没启动（Pending）。`
      : `${p.pod} 还没启动（Pending）。`,
  "sentence.diagnosis.unexplained.cause.notReady": (p) => {
    const state = p.check ? `在运行，但没有通过就绪检查（${p.check}）` : "在运行，但还没就绪"
    return p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} ${state}。`
      : `${p.pod} ${state}。`
  },
  "sentence.diagnosis.unexplained.cause.phase": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了：新副本 ${p.pod} 的状态是 ${p.phase}。`
      : `${p.pod} 的状态是 ${p.phase}。`,
  "sentence.diagnosis.unexplained.cause.none": (p) =>
    p.stuck === "true"
      ? `${p.service} 的部署卡住了。`
      : p.reason
        ? `${p.service} 异常（${p.reason}）。`
        : `${p.service} 异常。`,
  "sentence.diagnosis.unexplained.why.event": (p) =>
    `关于它的最新事件：${p.event}，“${p.note}”。${servingZhCN(p)}`,
  "sentence.diagnosis.unexplained.why.noEvent": (p) => `集群没有关于它的事件。${servingZhCN(p)}`,
  "sentence.diagnosis.unexplained.fix.waiting": (p) =>
    `打开 ${p.pod} 的事件，kubelet 会在那里说明它在等什么。`,
  "sentence.diagnosis.unexplained.fix.terminated": (p) =>
    `从 ${p.pod} 的日志和事件找出它停止的原因。`,
  "sentence.diagnosis.unexplained.fix.pending": (p) =>
    `打开 ${p.pod} 的事件，调度器和 kubelet 会在那里说明它为什么还没启动。`,
  "sentence.diagnosis.unexplained.fix.notReady": (p) =>
    `确认应用在就绪检查查看的位置有响应，再看 ${p.pod} 的日志。`,
  "sentence.diagnosis.unexplained.fix.phase": (p) => `查看 ${p.pod} 的事件和日志。`,
  "sentence.diagnosis.unexplained.fix.none": (p) => `查看 ${p.service} 的事件。`,
  "sentence.problem.summary.bad-request": () => "这个请求无效。",
  "sentence.problem.summary.unauthenticated": () => "集群不接受你的登录。",
  "sentence.problem.summary.signed-out": () => "你已退出登录。",
  "sentence.problem.summary.forbidden": (p) =>
    p.verb ? `你不能${verbZhCN[p.verb] ?? p.verb} ${thingZh(p, "里的")}。` : "你没有权限。",
  "sentence.problem.summary.elevation-required": (p) =>
    p.verb
      ? `你平时的权限不能${verbZhCN[p.verb] ?? p.verb} ${thingZh(p, "里的")}。`
      : "这需要管理员权限。",
  "sentence.problem.summary.not-found": (p) => (p.name ? `${p.name} 不存在。` : "它不存在。"),
  "sentence.problem.summary.log-unavailable": () => "无法读取这份日志。",
  "sentence.problem.summary.conflict": (p) =>
    p.name ? `你预览之后，${p.name} 又有了更改。` : "你预览之后，它又有了更改。",
  "sentence.problem.summary.managed": (p) =>
    p.name ? `${p.name} 由其他工具管理。` : "这由其他工具管理。",
  "sentence.problem.summary.plan-expired": () => "计划已过期。",
  "sentence.problem.summary.system-namespace": (p) =>
    p.name ? `${p.name} 是 Kubernetes 自用的 Namespace。` : "这是 Kubernetes 自用的 Namespace。",
  "sentence.problem.summary.invalid": (p) =>
    p.name ? `集群拒绝了对 ${p.name} 的更改。` : "集群拒绝了这项更改。",
  "sentence.problem.summary.too-many-requests": () => "集群正忙。",
  "sentence.problem.summary.unavailable": () => "连不上集群。",
  "sentence.problem.summary.internal": () => "Zeabur 内部发生错误。",
  "sentence.problem.summary.quota-exceeded": (p) =>
    p.name ? `更改 ${p.name} 会超过项目的用量上限。` : "这会超过项目的用量上限。",
  "sentence.problem.summary.policy-denied": (p) =>
    p.name ? `集群里的策略不允许更改 ${p.name}。` : "集群里的策略不允许这项更改。",
  "sentence.problem.why.quota-exceeded": (p) =>
    p.resource
      ? `项目用量上限 ${p.quota} 允许 ${p.resource} 最多 ${p.hard}，目前已用 ${p.used}。`
      : `项目用量上限 ${p.quota} 已经没有空间。`,
  "sentence.problem.why.policy-denied": (p) =>
    p.policy ? `策略 ${p.policy} 的回应是：` : "集群的回应是：",
  "sentence.diagnosis.crashLoop.cause.exitCode": (p) => `${p.container} 反复崩溃、重启。`,
  "sentence.diagnosis.crashLoop.cause.exitsZero": (p) => `${p.container} 反复正常退出后又重启。`,
  "sentence.diagnosis.crashLoop.cause.notExecutable": (p) =>
    `${p.container} 无法运行它的命令：这个文件不能执行。`,
  "sentence.diagnosis.crashLoop.cause.commandNotFound": (p) =>
    `${p.container} 无法运行它的命令：镜像里没有这个命令。`,
  "sentence.diagnosis.crashLoop.cause.killed": (p) => `${p.container} 反复被强制终止、重启。`,
  "sentence.diagnosis.crashLoop.cause.terminated": (p) => `${p.container} 反复收到停止请求后重启。`,
  "sentence.diagnosis.crashLoop.fix.exitsZero": () =>
    "服务必须持续运行；一次性的工作改用 Job 运行。",
  "sentence.diagnosis.crashLoop.fix.command": () => "检查镜像的 entrypoint、command 与 args。",
  "sentence.diagnosis.crashLoop.fix.killed": () => "有东西终止了它：检查内存用量与存活检查。",
  "sentence.diagnosis.crashLoop.fix.terminated": () => "检查存活检查，以及是什么让程序停止。",
  "sentence.diagnosis.crashLoop.fix.logs": () => "错误信息在重启前那一次的日志里。",
  "sentence.diagnosis.runFailed.fix.logs": () => "错误信息在失败那次运行的日志里。",
  "sentence.diagnosis.runFailed.cause.exitCode": (p) => `${p.container} 上一次运行失败。`,
  "sentence.diagnosis.runFailed.cause.exitsZero": (p) => `${p.container} 上一次运行意外结束。`,
  "sentence.diagnosis.runFailed.cause.notExecutable": (p) =>
    `${p.container} 上一次运行无法启动命令：这个文件不能执行。`,
  "sentence.diagnosis.runFailed.cause.commandNotFound": (p) =>
    `${p.container} 上一次运行在镜像里找不到命令。`,
  "sentence.diagnosis.runFailed.cause.killed": (p) => `${p.container} 上一次运行被强制终止。`,
  "sentence.diagnosis.runFailed.cause.terminated": (p) =>
    `${p.container} 上一次运行收到停止请求而结束。`,
  "sentence.diagnosis.outOfMemory.cause.limit": (p) => `${p.container} 用完了内存，系统终止了它。`,
  "sentence.diagnosis.outOfMemory.cause.noLimit": (p) =>
    `机器的内存不足，系统终止了 ${p.container}。`,
  "sentence.diagnosis.outOfMemory.fix.limit": () => "调高内存上限，或找出用掉内存的地方。",
  "sentence.diagnosis.outOfMemory.fix.noLimit": () => "按实际用量设置内存的预留量与上限。",
  "sentence.diagnosis.imagePull.cause.notFound": (p) =>
    `${p.container} 无法下载镜像 ${p.image ?? ""}：镜像仓库里没有这个镜像。`,
  "sentence.diagnosis.imagePull.cause.notFoundOrPrivate": (p) =>
    `${p.container} 无法下载镜像 ${p.image ?? ""}。`,
  "sentence.diagnosis.imagePull.cause.unauthorized": (p) =>
    `${p.container} 无法下载镜像：镜像仓库拒绝了登录信息。`,
  "sentence.diagnosis.imagePull.cause.rateLimited": (p) =>
    `${p.container} 无法下载镜像：超出了镜像仓库的下载次数限制。`,
  "sentence.diagnosis.imagePull.cause.unreachable": (p) =>
    `${p.container} 无法下载镜像：连不上镜像仓库。`,
  "sentence.diagnosis.imagePull.cause.invalidName": (p) =>
    `${p.container} 的镜像名称 ${p.image ?? ""} 有误。`,
  "sentence.diagnosis.imagePull.cause.unknown": (p) =>
    `${p.container} 无法下载镜像 ${p.image ?? ""}。`,
  "sentence.diagnosis.imagePull.fix.notFound": () => "确认镜像的名称与版本标签。",
  "sentence.diagnosis.imagePull.fix.notFoundOrPrivate": () =>
    "确认名称与版本标签；如果是私有镜像，为项目加上镜像仓库的登录信息。",
  "sentence.diagnosis.imagePull.fix.unauthorized": () =>
    "为项目加上镜像仓库的登录信息（imagePullSecret），或修正现有的。",
  "sentence.diagnosis.imagePull.fix.rateLimited": () => "登录镜像仓库，或改用镜像站。",
  "sentence.diagnosis.imagePull.fix.unreachable": () => "检查机器的网络与 DNS。",
  "sentence.diagnosis.imagePull.fix.invalidName": () => "修正镜像名称。",
  "sentence.diagnosis.imagePull.fix.unknown": () => "副本的事件里有镜像仓库的响应。",
  "sentence.diagnosis.probeFailure.cause.liveness": (p) =>
    `${p.container} 的存活检查失败 ${p.count} 次${p.output ? `：${p.output}` : "。"}`,
  "sentence.diagnosis.probeFailure.cause.readiness": (p) =>
    `${p.container} 的就绪检查失败 ${p.count} 次${p.output ? `：${p.output}` : "。"}`,
  "sentence.diagnosis.probeFailure.cause.startup": (p) => `${p.container} 没有在时限内完成启动。`,
  "sentence.diagnosis.probeFailure.fix.liveness": () =>
    "到健康检查确认存活检查的路径、端口与时间设置。",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "到健康检查确认就绪检查的路径、端口与时间设置。",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "如果只是启动得比较慢，到健康检查放宽时间；如果早该启动好了，日志会说明原因。",
  "sentence.gap.resourceLimits.none": (p) =>
    `${p.container} 没有设置 CPU 与内存的 request 或 limit。`,
  "sentence.gap.resourceLimits.partial": (p) =>
    `${p.container} 没有设置 ${(p.missing ?? "").split(",").join("、")}。`,
  "sentence.gap.healthChecks.none": (p) => `${p.container} 没有就绪检查，未就绪时也可能收到流量。`,
  "sentence.gap.healthChecks.livenessOnly": (p) => `${p.container} 有存活检查，但没有就绪检查。`,
  "sentence.gap.ingressToHttpRoute.retired": (p) =>
    `Ingress ${p.ingress} 使用 ${p.class}，它的 controller 已停止维护。`,
  "sentence.gap.ingressToHttpRoute.available": (p) =>
    `集群已有 Gateway API，Ingress ${p.ingress} 可以改用 HTTPRoute。`,
  "sentence.gap.retiredIngressController": (p) =>
    `IngressClass ${p.class} 的 controller ${p.controller} 已停止维护。`,
  "sentence.gap.danglingRoute.noService": (p) =>
    `${p.kind} ${p.route} 指定的 Service ${(p.services ?? "").split(",").join("、")} 不在这个项目里。`,
  "sentence.gap.danglingRoute.noGateway": (p) =>
    `HTTPRoute ${p.route} 指定的 Gateway ${(p.gateways ?? "").split(",").join("、")} 不存在。`,
  "sentence.gap.danglingRoute.noParent": (p) =>
    `HTTPRoute ${p.route} 没有指定 Gateway，所以它收不到任何流量。`,
  "sentence.gap.danglingRoute.notAccepted": (p) =>
    `没有任何 Gateway 接受 HTTPRoute ${p.route}${p.reason ? `（${p.reason}）` : ""}，所以它收不到任何流量。`,
  "sentence.gap.blockedConnection": (p) =>
    p.at === "egress"
      ? `${p.from} 的环境变量指向 ${p.to}，但 ${p.from} 的 NetworkPolicy 不允许连出。`
      : p.at === "both"
        ? `${p.from} 的环境变量指向 ${p.to}，但两端的 NetworkPolicy 都拦截了这条连接。`
        : `${p.from} 的环境变量指向 ${p.to}，但 ${p.to} 的 NetworkPolicy 不允许 ${p.from} 连入。`,
  "sentence.gap.noNetworkPolicy": (p) => {
    const blocked = cuts(
      p,
      {
        otherNamespaces: "来自其他 Namespace 的连接",
        outside: "连到集群外的连接",
        published: "NodePort 与 LoadBalancer 的流量",
        connections: "服务之间的一条连接",
      },
      (items) => items.join("、"),
    )
    return [
      `没有 NetworkPolicy 保护 ${(p.services ?? "").split(",").join("、")}：集群里任何 Pod 都连得到。`,
      p.enforcement === "unknown" ? "NetworkPolicy 要网络插件（CNI）支持才会生效。" : "",
      blocked ? `隔离后也会拦截${latinStart(blocked)}${blocked}。` : "",
    ].join("")
  },
  "sentence.gap.volumeWithoutBackup": (p) => `卷 ${p.claim} 没有备份计划。`,
  "sentence.gap.databaseWithoutBackup": (p) => `数据库 ${p.database} 没有备份计划。`,
  "sentence.page.noAnswer": () => "Zeabur 没有响应。",
  "sentence.page.status": (p) => `请求失败，HTTP 状态码 ${p.status}。`,
  "sentence.page.reload": () => "确认 Zeabur 仍在运行，然后刷新页面。",
  "sentence.page.followFailed": () => "无法实时跟踪日志。",
  "sentence.page.followOff": () => "关闭“实时跟踪”，快照会显示集群的响应。",
  "status.readyUnchecked": "运行中（无就绪检查）",
  "origin.code": "应用本身",
  "origin.configuration": "服务配置",
  "origin.cluster": "集群",
  "origin.upstream": "集群外部",
  "origin.unknown": "其他",
  "origin.code.hint": "应用本身的代码或命令让它停止；日志会说明原因。",
  "origin.configuration.hint": "这个服务的某项配置导致问题：变量、上限、检查或地址。",
  "origin.cluster.hint": "服务底下的集群导致问题：节点、配额或剩余资源。",
  "origin.upstream.hint": "集群外部的东西导致问题，例如镜像仓库。",
  "origin.unknown.hint": "Zeabur 还没把它归到已知的原因；上面的句子写着集群报告的状态。",
  "fixer.you": "你可以修复",
  "fixer.clusterAdmin": "需要集群管理员",
  "fixer.unknown": "不确定由谁修复",
  "diagnosis.copy": "复制诊断",
  "diagnosis.copyFailed": "无法复制诊断，请重试。",
  "diagnosis.lastSeen": "最后出现",
  "trace.title": "连接检查",
  "trace.reachable": "可访问",
  "trace.unreachable": "无法访问",
  "trace.unverified": "无法从集群内确认",
  "trace.howToFix": "修复方法",
  "trace.showChecks": "显示连接检查",
  "trace.hop.ok": "通过",
  "trace.hop.failed": "失败",
  "trace.hop.unknown": "未检查",
  "status.uncheckedWhy": "容器已经启动，但没有检查它能不能响应请求。",
  "status.addTrafficCheck": "添加就绪检查",
  "status.private": "私有",
  "rollout.title": "部署进度",
  "rollout.ready": (ready, desired) => `${desired} 个新副本中 ${ready} 个就绪`,
  "rollout.older": (n) => `${n} 个旧副本仍在运行`,
  "rollout.stuckLine": "部署卡住",
  "rollout.waitsLine": "新版本要等这个问题解决",
  "rollout.rollingOut": (name) => `${name} 正在部署`,
  "overview.alsoMeans": "连带影响",
  "overview.alsoDown": (list, name) => `${name} 有就绪的副本之前，无法访问 ${list}。`,
  "overview.privateAddress": "私有地址",
  "trace.waiting": (name) => `等待 ${name} 就绪`,
  "trace.stopped": (name) => `${name} 目前是 0 个副本`,
  "event.word.retrying": "重试等待中",
  "event.word.checkFailed": "检查未通过",
  "event.word.stopping": "正在停止",
  "event.word.failed": "失败",
  "event.word.noRoom": "等待可用资源",
  "event.word.mountFailed": "无法挂载卷",
  "event.word.createFailed": "无法创建副本",
  "event.word.gaveUp": "已放弃重试",
  "event.word.evicted": "已移出节点",
  "event.word.outOfMemory": "内存不足",
  "changes.title": "历次部署",
  "changes.description":
    "服务的每个发布版本，以及前后发生的警告。可以把某个版本和当前版本比较，或回滚到该版本。",
  "changes.none": "还没有部署记录",
  "changes.release": (revision) => `修订版本 #${revision}`,
  "settings.sections": "这个服务的设置",
  "part.variables": "变量",
  "part.networking": "网络",
  "part.storage": "存储",
  "part.compute": "计算资源",
  "part.health-checks": "健康检查",
  "form.healthHint": "集群在每个副本上运行的检查：能不能接收流量、要不要重启，以及是否已经启动。",
  "draft.add": "加入变更",
  "draft.pendingHere": "有等待预览的变更",
  "draft.count": (n) => `${n} 项变更待应用`,
  "draft.review": "预览变更",
  "draft.discard": "放弃",
  "draft.title": (name) => `变更 ${name}`,
  "project.view": "视图",
  "project.viewMap": "架构图",
  "project.viewList": "列表",
  "project.suggestions": (n) => `${n} 项建议`,
  "project.suggestionsBody":
    "这些服务目前缺少的配置。它们不会让服务现在停止，但补上之后，问题更容易被发现或避免。",
  "project.activity": "动态",
  "project.next.title": "部署第一个服务",
  "project.next.body":
    "只要这个命名空间里有工作负载在运行，服务就会出现在这里：Deployment、StatefulSet、DaemonSet、Job 或 CronJob。",
  "project.next.command": "用示例 Web 服务器试试",
  "project.next.after": "用 kubectl 运行，或应用你自己的 YAML。页面会自动更新。",
  "freshness.failed": "无法刷新",
  "freshness.lastRead": "最后读取",
  "home.state.fine": (services, projects) =>
    `${projects} 个项目中的 ${services} 个服务都在正常运行。`,
  "home.state.problems": (n) => `有 ${n} 个问题需要处理。`,
  "home.state.rolling": (n) => `${n} 个服务正在部署。`,
  "home.affects": "影响",
  "home.inProgress": "进行中",
  "home.problems": "需要你处理",
  "shell.context": (cluster, user) => `集群 ${cluster}，登录身份 ${user}`,
  "home.clusterNotices": (n) => `${n} 条集群提醒，目前没有服务因此停摆：`,
  "origin.component": "集群组件",
  "origin.component.hint": "集群为所有项目运行的组件导致问题，例如 Ingress 控制器或集群 DNS。",
  "origin.zeabur": "Zeabur 本身",
  "origin.zeabur.hint": "Zeabur 本身导致问题；请附上复制的诊断反馈。",
  "fixer.componentOwner": "需要组件负责人",
  "incident.title": "集群",
  "incident.hint": "一个原因让好几个服务停摆；修好它，这些服务就会一起恢复。",
  "incident.domains": (n) => `${n} 个域名`,
  "incident.causedBy": "起因",
  "timeline.truncated": "只显示最新的 200 条变更，更早的没有列出。",
  "timeline.empty": "最近没有变更。集群只保留约一小时的事件，以及每个工具最后一次的写入。",
  "timeline.by": (who) => `由 ${who}`,
  "timeline.tool.zeabur": "Zeabur",
  "timeline.tool.helm": "Helm",
  "timeline.tool.argoCd": "Argo CD",
  "timeline.tool.flux": "Flux",
  "timeline.tool.kubectl": "kubectl",
  "timeline.tool.autoscaler": "自动扩缩",
  "trace.step.dns": "DNS",
  "trace.step.address": "入口地址",
  "trace.step.controller": "Ingress 控制器",
  "trace.step.certificate": "证书",
  "trace.step.route": "路由",
  "trace.step.service": "私有地址",
  "trace.step.targetPort": "端口",
  "trace.step.ready": "就绪副本",
  "trace.expected": "预期",
  "trace.saw": "实际",
  "trace.fix": "修复",
  "trace.fixTitle": (address) => `修复通往 ${address} 的路径`,
  "rollout.rollBack": (name) => `回滚 ${name}`,
  "rollout.rollBackButton": "回滚",
  "sentence.incident.componentDown.cause.ingressController": (p) =>
    `Ingress 控制器 ${p.workload} 停摆了：它没有任何就绪的副本。`,
  "sentence.incident.componentDown.cause.certManager": (p) =>
    `cert-manager 停摆了：${p.workload} 没有任何就绪的副本。`,
  "sentence.incident.componentDown.cause.clusterDns": (p) =>
    `集群 DNS 停摆了：${p.workload} 没有任何就绪的副本。`,
  "sentence.incident.componentDown.cause.metricsServer": (p) =>
    `metrics-server 停摆了：${p.workload} 没有任何就绪的副本。`,
  "sentence.incident.componentDown.why.ingressController": (p) =>
    `Ingress 控制器负责把公开域名的请求送到对应的服务；它停摆时，${p.domains} 个域名和 ${p.services} 个服务收不到集群外部的请求。`,
  "sentence.incident.componentDown.why.certManager": (p) =>
    `cert-manager 负责签发和续期域名要求的证书；它停摆时，${p.domains} 个域名的证书不会签发也不会续期，证书一过期，浏览器就会拒绝。`,
  "sentence.incident.componentDown.why.clusterDns": (p) =>
    `服务之间靠集群 DNS 用名称找到彼此；它停摆时，有 ${p.services} 个服务用名称连接，会连不上对方。`,
  "sentence.incident.componentDown.why.metricsServer": () =>
    "metrics-server 负责测量每个副本的用量；它停摆时，看不到用量数据，自动扩缩容也会停止。",
  "sentence.incident.componentDown.fix.ingressController": (p) =>
    `检查 ${p.namespace ? `${p.namespace} 里的 ` : ""}${p.workload} 为什么没有就绪的副本；它的事件和日志会说明原因。`,
  "sentence.incident.componentDown.fix.certManager": (p) =>
    `检查 ${p.namespace ? `${p.namespace} 里的 ` : ""}${p.workload} 为什么没有就绪的副本；它的事件和日志会说明原因。`,
  "sentence.incident.componentDown.fix.clusterDns": (p) =>
    `检查 ${p.namespace ? `${p.namespace} 里的 ` : ""}${p.workload} 为什么没有就绪的副本；它的事件和日志会说明原因。`,
  "sentence.incident.componentDown.fix.metricsServer": (p) =>
    `检查 ${p.namespace ? `${p.namespace} 里的 ` : ""}${p.workload} 为什么没有就绪的副本；它的事件和日志会说明原因。`,
  "sentence.incident.nodeNotReady.cause": (p) =>
    `${p.status === "Unknown" ? `节点 ${p.node} 不再向集群报告状态` : `节点 ${p.node} 未就绪`}${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.incident.nodeNotReady.why": (p) =>
    `节点未就绪时不会再启动新的 Pod；宽限期（默认 5 分钟）过后，集群会移除上面的 Pod，让它们到别处启动。这个节点上有 ${p.pods} 个 Pod，${p.services} 个服务有副本在上面。`,
  "sentence.incident.nodeNotReady.fix": (p) =>
    `检查 ${p.node} 背后的机器：是否开机、网络，以及节点代理（kubelet）。如果它无法恢复，就清空这个节点并替换它。`,
  "sentence.trace.dns.ok": (p) => `${p.host} 可以解析。`,
  "sentence.trace.dns.notFound": (p) =>
    `从集群内解析不到 ${p.host}；其他地方的 DNS 可能仍有响应，所以 Zeabur 无法判断。`,
  "sentence.trace.dns.unknown": (p) => `没有查询 ${p.host}，或查询没有及时响应。`,
  "sentence.trace.address.noStatus": (p) => `${p.route} 的状态里还没有地址，无法和域名的地址比对。`,
  "sentence.trace.address.unresolved": (p) => `域名没有解析出地址，无法和 ${p.route} 的地址比对。`,
  "sentence.trace.address.ok": (p) => `域名指向 ${p.route} 提供服务的地址。`,
  "sentence.trace.address.elsewhere": (p) =>
    `域名指向的地址不是 ${p.route} 的；集群前面的 CDN 或代理服务器会用自己的地址响应。`,
  "sentence.trace.controller.classesUnreadable": (p) =>
    `Zeabur 无法读取集群的 IngressClass，因此无法判断哪个控制器负责 ${p.route}。`,
  "sentence.trace.controller.classMissing": (p) =>
    `${p.route} 指定的 IngressClass ${p.class} 不在集群里。`,
  "sentence.trace.controller.unreadable": (p) =>
    `Zeabur 无法读取集群的组件，因此无法判断 ${p.route} 的 Ingress 控制器是否在运行。`,
  "sentence.trace.controller.notRecognised": (p) =>
    `Zeabur 识别不出负责 ${p.route} 的 Ingress 控制器${p.controller ? ` ${p.controller}` : ""}，因此无法判断它是否在运行。`,
  "sentence.trace.controller.ok": (p) => `Ingress 控制器 ${p.workload} 有就绪的副本。`,
  "sentence.trace.controller.down": (p) =>
    `Ingress 控制器 ${p.workload} 没有就绪的副本，所以没有东西处理 ${p.route}。`,
  "sentence.trace.certificate.defaultCertificate": (p) =>
    `${p.route} 为 ${p.host} 启用 HTTPS，却没有指定机密，所以控制器会提供它的默认证书。`,
  "sentence.trace.certificate.ok": (p) =>
    `${p.host} 的证书已签发${p.until ? `，有效期至 ${p.until}` : ""}。`,
  "sentence.trace.certificate.notReady": (p) =>
    `${p.host} 的证书 ${p.certificate} 尚未签发${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.trace.certificate.pending": (p) => `${p.host} 的证书 ${p.certificate} 还没有结果。`,
  "sentence.trace.certificate.unreadable": (p) =>
    `Zeabur 无法读取项目的机密，因此无法检查机密 ${p.secret} 里 ${p.host} 的证书。`,
  "sentence.trace.certificate.notChecked": (p) =>
    `机密 ${p.secret} 存有 ${p.host} 的证书；Zeabur 不会进一步检查。`,
  "sentence.trace.certificate.secretMissing": (p) =>
    `${p.route} 从机密 ${p.secret} 读取 ${p.host} 的证书，但项目里没有这个机密；控制器可能改用默认证书，Zeabur 无法检查它。`,
  "sentence.trace.route.noStatus": (p) =>
    `还没有任何组件写入 ${p.route} 的状态，无法得知它是否已生效。`,
  "sentence.trace.route.ok": (p) => `${p.route} 已生效：控制器已在它的状态里写入地址。`,
  "sentence.trace.route.accepted": (p) => `Gateway 已接受 ${p.route}。`,
  "sentence.trace.route.notAccepted": (p) =>
    `${p.route} 的流量入口报告 ${p.condition} ${p.reason}；请求无法通过。`,
  "sentence.trace.service.unreadable": (p) =>
    `Zeabur 无法读取项目的私有地址（Service），因此无法检查 ${p.endpoint}。`,
  "sentence.trace.service.missing": (p) => `找不到名为 ${p.endpoint} 的私有地址（Service）。`,
  "sentence.trace.service.portMissing": (p) => `私有地址 ${p.endpoint} 没有端口 ${p.port}。`,
  "sentence.trace.service.selectsOther": (p) =>
    `私有地址 ${p.endpoint} 把流量发到其他副本，而不是 ${p.service} 的副本。`,
  "sentence.trace.service.ok": (p) => `私有地址 ${p.endpoint} 把流量发到 ${p.service}。`,
  "sentence.trace.targetPort.unknown": (p) =>
    `Zeabur 读不到容器，因此没有检查 ${p.endpoint} 转发到的端口。`,
  "sentence.trace.targetPort.notReached": (p) =>
    `没有检查：流量在到达 ${p.endpoint} 的端口之前就停了。`,
  "sentence.trace.targetPort.mismatch": (p) =>
    `${p.endpoint} 把流量送到端口 ${p.targetPort}，但容器没有声明这个端口${p.ports ? `（声明的是 ${p.ports}）` : ""}。`,
  "sentence.trace.targetPort.undeclared": (p) =>
    `容器没有声明任何端口，无法得知它们是否在 ${p.endpoint} 转发到的端口上监听。`,
  "sentence.trace.targetPort.ok": (p) => `${p.endpoint} 把流量送到容器声明的端口。`,
  "sentence.trace.ready.scaledToZero": (p) => `${p.service} 的副本数设为 0。`,
  "sentence.trace.ready.ok": (p) => `${p.service} 有 ${p.ready}/${p.pods} 个副本就绪。`,
  "sentence.trace.ready.starting": (p) => `${p.service} 正在启动，还没有就绪的副本。`,
  "sentence.trace.ready.noneReady": (p) => `${p.service} 没有任何就绪的副本。`,
  "sentence.diagnosis.unreachable.cause.ingressClassMissing": (p) =>
    `发往 ${p.route} 域名的请求到不了 ${p.service}：它指定的 IngressClass ${p.class} 不在集群里。`,
  "sentence.diagnosis.unreachable.why.ingressClassMissing": () =>
    "Ingress 控制器只处理自己类别的 Ingress；没有这个类别，就没有控制器处理它。",
  "sentence.diagnosis.unreachable.fix.ingressClassMissing": (p) =>
    `把 ${p.route} 的类别改成集群里有的类别，或安装 ${p.class} 的控制器。`,
  "sentence.diagnosis.unreachable.cause.controllerDown": (p) =>
    `发往 ${p.route} 域名的请求到不了 ${p.service}：Ingress 控制器 ${p.workload} 没有就绪的副本。`,
  "sentence.diagnosis.unreachable.why.controllerDown": () =>
    "Ingress 控制器负责把域名的请求带进集群；它没有任何就绪的副本时，就没有东西响应这些请求。",
  "sentence.diagnosis.unreachable.fix.controllerDown": (p) =>
    `检查 ${p.namespace ? `${p.namespace} 里的 ` : ""}${p.workload} 为什么没有就绪的副本；它的事件和日志会说明原因。`,
  "sentence.diagnosis.unreachable.cause.certificateNotReady": (p) =>
    `${p.host} 的 HTTPS 连接失败：证书 ${p.certificate} 尚未签发${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.diagnosis.unreachable.why.certificateNotReady": (p) =>
    `cert-manager 签发证书后才会把它写进机密 ${p.secret}；在那之前，控制器会改用默认证书，浏览器会拒绝它。`,
  "sentence.diagnosis.unreachable.fix.certificateNotReady": (p) =>
    `证书 ${p.certificate} 的状态和事件会说明它为什么还没签发。`,
  "sentence.diagnosis.unreachable.cause.backendPortMissing": (p) =>
    `通往 ${p.service} 的路由指定了端口 ${p.port}，但它的私有地址 ${p.endpoint} 没有这个端口。`,
  "sentence.diagnosis.unreachable.why.backendPortMissing": () =>
    "路由会用编号或名称把流量发到私有地址的某个端口；私有地址没有的端口收不到任何流量。",
  "sentence.diagnosis.unreachable.fix.backendPortMissing": (p) =>
    `让路由指向 ${p.endpoint} 有的端口，或在 ${p.endpoint} 加上端口 ${p.port}。`,
  "sentence.diagnosis.unreachable.cause.selectsOther": (p) =>
    `通往 ${p.service} 的路由把流量发到 ${p.endpoint}，但 ${p.endpoint} 会发到其他副本，而不是 ${p.service} 的副本。`,
  "sentence.diagnosis.unreachable.why.selectsOther": () =>
    "私有地址只会把流量发到标签符合它 selector 的副本。",
  "sentence.diagnosis.unreachable.fix.selectsOther": (p) =>
    `让 ${p.endpoint} 的 selector 匹配 ${p.service} 的标签，或让路由改为指向 ${p.service} 自己的私有地址。`,
  "sentence.diagnosis.unreachable.cause.scaledToZero": (p) =>
    `没有流量到得了 ${p.service}：它的副本数设为 0。`,
  "sentence.diagnosis.unreachable.why.scaledToZero": () =>
    "私有地址只会把流量发到运行中的副本，而目前配置为不运行任何副本。",
  "sentence.diagnosis.unreachable.fix.scaledToZero": (p) => `把 ${p.service} 的副本数调到至少 1。`,
  "sentence.timeline.release": (p) =>
    `修订版本 #${p.revision}${p.images ? ` 使用 ${p.images}` : ""}${p.changeCause ? `（${p.changeCause}）` : ""}。`,
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
    "自动扩缩容（HorizontalPodAutoscaler）最近一次修改了副本数。",
  "sentence.timeline.write.controller": (p) =>
    `Kubernetes（${p.manager}）最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.write.other": (p) =>
    `${p.manager} 最近一次修改${p.fields ? ` ${p.fields}` : "它"}。`,
  "sentence.timeline.warning.crashLoop": (p) => `${p.object} 反复崩溃、重启（${p.count} 次）。`,
  "sentence.timeline.warning.evicted": (p) =>
    `${p.object} 被移出所在的节点（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.unschedulable": (p) =>
    `${p.object} 找不到可以运行的机器（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.imagePull": (p) =>
    `${p.object} 无法下载镜像（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.probe": (p) =>
    `${p.object} 的健康检查失败（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.quota": (p) =>
    `${p.object} 无法创建副本（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.warning.other": (p) =>
    `${p.object} 发生 ${p.reason}（${p.count} 次）${p.note ? `：${p.note}` : "。"}`,
  "sentence.timeline.termination.oomKilled": (p) =>
    `${p.pod} 里的 ${p.container} 用完了内存，系统终止了它。`,
  "sentence.timeline.termination.exited": (p) =>
    `${p.pod} 里的 ${p.container} 退出了，退出码 ${p.exitCode}${p.reason ? `（${p.reason}）` : ""}。`,
  "sentence.rollout.progressing": (p) =>
    `${p.name} 正在部署：${p.desired} 个副本中有 ${p.updated} 个是新的修订版本。`,
  "sentence.rollout.done": (p) => `${p.name} 已部署完成：每个副本都是当前的修订版本。`,
  "sentence.rollout.paused": (p) => `${p.name} 的部署已暂停；恢复后才会启动新的副本。`,
  "sentence.rollout.waitsForDeletion": (p) =>
    `${p.name} 只在副本被删除时才换成新版本（OnDelete）；${p.desired} 个中有 ${p.updated} 个是当前的修订版本。`,
  "sentence.rollout.stuck.crashLoop": (p) => `${p.name} 的部署卡住了：新副本反复崩溃。`,
  "sentence.rollout.stuck.outOfMemory": (p) => `${p.name} 的部署卡住了：新副本用完了内存。`,
  "sentence.rollout.stuck.imagePull": (p) => `${p.name} 的部署卡住了：新副本无法下载镜像。`,
  "sentence.rollout.stuck.unschedulable": (p) =>
    `${p.name} 的部署卡住了：新副本正在等待可以运行的机器。`,
  "sentence.rollout.stuck.quota": (p) => `${p.name} 的部署卡住了：项目用量上限不允许新增副本。`,
  "sentence.rollout.stuck.configMissing": (p) =>
    `${p.name} 的部署卡住了：新副本读取的机密或配置文件不存在。`,
  "sentence.rollout.stuck.runFailed": (p) => `${p.name} 的部署卡住了：新副本运行失败。`,
  "sentence.rollout.stuck.deadlineExceeded": (p) =>
    `${p.name} 的部署卡住了：期限（progressDeadlineSeconds）内没有进展。`,
  "sentence.rollout.stuck.replicaFailure": (p) => `${p.name} 的部署卡住了：集群无法创建新副本。`,
  "sentence.gap.ephemeralData.none": (p) =>
    `${p.container} 把 ${p.software} 数据存在 ${p.path}，但这个路径不在任何卷上；副本一替换，数据就没了。`,
  "sentence.gap.ephemeralData.ephemeral": (p) =>
    `${p.container} 把 ${p.software} 数据存在 ${p.path}，但这个卷只随副本存在；副本一替换，数据就没了。`,
  // ia:start
  "preview.restarts": "会重新启动的服务",
  "preview.roll.rolling": (name) => `${name} 会逐一替换副本。`,
  "preview.roll.recreate": (name) => `${name} 会先停止所有副本，再启动新的。`,
  "preview.roll.onDelete": (name) => `${name} 要等你删除旧副本后才会应用新设置。`,
  "preview.scale": (name, from, to) => `${name} 从 ${from} 个副本变成 ${to} 个。`,
  "preview.start": (name) => `${name} 会启动。`,
  "preview.stop": (name) => `${name} 会停止。`,
  "preview.pendingRestart": (name) => `${name} 重新启动前会继续使用旧的值。`,
  "preview.downtime": "会有短暂的时间没有副本提供服务。",
  "preview.cores": (n) => `${n} 核 CPU`,
  "preview.fits": (pods, cpu, memory) =>
    `资源足够：峰值时会多出 ${pods} 个 Pod、${cpu} 和 ${memory}。`,
  "preview.blocked": "资源不足的原因",
  "preview.blockedQuota": (name, resource, need, left) =>
    `配额 ${name} 不够：${resource} 需要 ${need}，只剩 ${left}。`,
  "preview.blockedNodes": (_name, resource, need, left) =>
    `没有节点放得下：${resource} 需要 ${need}，最空的节点只有 ${left}。`,
  "preview.fitUnknown": "Zeabur 无法确认新的 Pod 是否放得下。",
  "preview.dataLoss": "会丢失的数据",
  "preview.loss.claimDeleted": (claim, paths) =>
    `会删除磁盘 ${claim}${paths ? `，以及 ${paths} 里的数据` : ""}。`,
  "preview.loss.emptyDir": (name, volume, paths) =>
    `${name} 把 ${paths || volume} 放在临时卷；替换副本时会清空。`,
  "preview.loss.statefulSet": (name, claim) =>
    `${name} 会一并删除它从 ${claim} 创建的磁盘和里面的数据。`,
  "preview.removals": "会移除的设置",
  "preview.removal": (path, name) => `会从 ${name} 移除 ${path}，当前是`,
  "preview.leftOut": "（你编辑时没有保留）",
  "env.pendingRestart": "重新启动前可能仍在使用旧的值",
  "env.pendingRestartDetail": (pod) => `这个值在 ${pod} 启动后才变更。重新启动服务才会使用新的值。`,
  // ia:end
}
