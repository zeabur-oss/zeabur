import { durationOf, items, joinWords, type P } from "@/i18n/sentence-parts"

const seconds = (n: number) => `${n} 秒`
const minutes = (n: number) => `${n} 分钟`
const words = (list: string[]) => joinWords(list, "、", "、")
const duration = (value: string | undefined) => durationOf(value, seconds, minutes)

export const serverZhCn = {
  "sentence.diagnosis.crashLoop.why.backOff": (p: P) =>
    `它已重启 ${p.restarts ?? ""} 次。集群每次都会再启动它，两次之间等得越来越久，默认最多等 5 分钟。`,
  "sentence.diagnosis.crashLoop.why.exitsZero": (p: P) =>
    `服务应该持续运行，所以程序一结束，即使是正常结束，集群也会再启动它（目前 ${p.restarts ?? ""} 次）。`,
  "sentence.diagnosis.runFailed.why.exitCode": (p: P) =>
    `程序的退出码不是 0，就算运行失败。这次的退出码是 ${p.exitCode ?? ""}。`,
  "sentence.diagnosis.runFailed.why.gaveUp": (p: P) =>
    `${!p.backoffLimit || p.backoffLimit === "none" ? `这项任务重试后已放弃，最后一次的退出码是 ${p.exitCode ?? ""}。` : `${p.backoffLimit === "0" ? `这项任务失败后不会重试，这次的退出码是 ${p.exitCode ?? ""}。` : `这项任务已放弃：运行失败时会重试 ${p.backoffLimit ?? ""} 次，最后一次的退出码也是 ${p.exitCode ?? ""}。`}`}`,
  "sentence.diagnosis.outOfMemory.why.limit": (p: P) =>
    `每个副本最多只能用 ${p.limit ?? ""} 内存，用到上限时系统会直接终止它，程序来不及处理。`,
  "sentence.diagnosis.outOfMemory.why.noLimit": () =>
    "没有内存上限时，它可以一直用到机器的内存不足，这时系统会终止用得最多的程序。",
  "sentence.diagnosis.imagePull.why": () =>
    "集群要先下载镜像才能启动容器。下载失败时会一再重试，每次等得更久。",
  "sentence.diagnosis.probeFailure.why.liveness": (p: P) =>
    `${!p.period || p.period === "none" ? `存活检查连续失败几次后，集群就会重启容器。` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `默认情况下，集群每 ${duration(p.period)}检查一次，连续失败 ${p.threshold ?? ""} 次（约 ${duration(p.total)}）就会重启容器。` : `集群每 ${duration(p.period)}检查一次，连续失败 ${p.threshold ?? ""} 次（约 ${duration(p.total)}）就会重启容器。每 ${duration(p.period)}是默认值。`}` : `集群每 ${duration(p.period)}检查一次，连续失败 ${p.threshold ?? ""} 次（约 ${duration(p.total)}）就会重启容器。${p.thresholdDefault === "true" ? `${p.threshold ?? ""} 次是默认值。` : ``}`}`}`,
  "sentence.diagnosis.probeFailure.why.readiness": (p: P) =>
    `${!p.period || p.period === "none" ? `就绪检查失败期间，容器不会从私有地址收到流量。` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `默认情况下，集群每 ${duration(p.period)}检查一次，连续失败 ${p.threshold ?? ""} 次（约 ${duration(p.total)}）后，容器就不会从私有地址收到流量，直到再次通过。` : `集群每 ${duration(p.period)}检查一次，连续失败 ${p.threshold ?? ""} 次（约 ${duration(p.total)}）后，容器就不会从私有地址收到流量，直到再次通过。每 ${duration(p.period)}是默认值。`}` : `集群每 ${duration(p.period)}检查一次，连续失败 ${p.threshold ?? ""} 次（约 ${duration(p.total)}）后，容器就不会从私有地址收到流量，直到再次通过。${p.thresholdDefault === "true" ? `${p.threshold ?? ""} 次是默认值。` : ``}`}`}`,
  "sentence.diagnosis.probeFailure.why.startup": (p: P) =>
    `${!p.period || p.period === "none" ? `其他检查会等启动检查通过，如果没有在时限内通过，集群就会重启容器。` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `默认情况下，容器有 ${p.threshold ?? ""} × ${duration(p.period)}（${duration(p.total)}）可以通过启动检查，超过时集群就会重启它。` : `容器有 ${p.threshold ?? ""} × ${duration(p.period)}（${duration(p.total)}）可以通过启动检查，超过时集群就会重启它。每 ${duration(p.period)}是默认值。`}` : `容器有 ${p.threshold ?? ""} × ${duration(p.period)}（${duration(p.total)}）可以通过启动检查，超过时集群就会重启它。${p.thresholdDefault === "true" ? `${p.threshold ?? ""} 次是默认值。` : ``}`}`}`,
  "sentence.diagnosis.configMissing.cause.object": (p: P) =>
    `${p.container ?? ""} 无法启动：${p.object === "Secret" ? `机密变量 ${p.name ?? ""}` : `配置文件 ${p.name ?? ""}`} 不存在。`,
  "sentence.diagnosis.configMissing.cause.key": (p: P) =>
    `${p.container ?? ""} 无法启动：${p.object === "Secret" ? `机密变量 ${p.name ?? ""}` : `配置文件 ${p.name ?? ""}`} 里没有 ${p.key ?? ""}。`,
  "sentence.diagnosis.configMissing.cause.unknown": (p: P) =>
    `${p.container ?? ""} 无法启动：缺少它读取的机密变量或配置文件。`,
  "sentence.diagnosis.configMissing.cause.volume": (p: P) =>
    `${!p.container || p.container === "none" ? `副本 ${p.pod ?? ""}` : `${p.container ?? ""}`} 无法启动：卷 ${p.volume ?? ""} 来自 ${p.object === "Secret" ? `机密变量 ${p.name ?? ""}` : `配置文件 ${p.name ?? ""}`}，但它不存在。`,
  "sentence.diagnosis.configMissing.why": () =>
    "容器读取的每个机密变量、配置文件和其中的键都存在之后，集群才会启动它。",
  "sentence.diagnosis.configMissing.why.volume": () =>
    "集群会先挂载所有卷，再启动副本的容器。卷如果来自不存在的机密变量或配置文件，就无法挂载，容器会一直等待。",
  "sentence.diagnosis.configMissing.fix.object": (p: P) =>
    `在项目里加上 ${p.object === "Secret" ? `机密变量 ${p.name ?? ""}` : `配置文件 ${p.name ?? ""}`}，或不再读取它。`,
  "sentence.diagnosis.configMissing.fix.key": (p: P) =>
    `在 ${p.object === "Secret" ? `机密变量 ${p.name ?? ""}` : `配置文件 ${p.name ?? ""}`} 加上 ${p.key ?? ""}，或不再读取它。`,
  "sentence.diagnosis.configMissing.fix.unknown": () =>
    "副本的事件会说缺少什么。把它加上，或不再读取它。",
  "sentence.diagnosis.configMissing.fix.volume": (p: P) =>
    `在项目里加上 ${p.object === "Secret" ? `机密变量 ${p.name ?? ""}` : `配置文件 ${p.name ?? ""}`}，或移除卷 ${p.volume ?? ""}。`,
  "sentence.diagnosis.unschedulable.cause.cpu": (p: P) =>
    `${p.service ?? ""} 正在等待可用资源：没有机器有足够的 CPU。`,
  "sentence.diagnosis.unschedulable.cause.memory": (p: P) =>
    `${p.service ?? ""} 正在等待可用资源：没有机器有足够的内存。`,
  "sentence.diagnosis.unschedulable.cause.placement": (p: P) =>
    `${p.service ?? ""} 正在等待可以运行它的机器。`,
  "sentence.diagnosis.unschedulable.cause.other": (p: P) =>
    `${p.service ?? ""} 正在等待可以运行的机器。`,
  "sentence.diagnosis.unschedulable.why": (p: P) =>
    `每个副本都要放在容得下它预留量的机器上。集群的说明：${p.message ?? ""}`,
  "sentence.diagnosis.unschedulable.cause.claimMissing": (p: P) =>
    `${p.service ?? ""} 无法启动：它要挂载的卷 ${p.claim ?? ""} 不存在。`,
  "sentence.diagnosis.unschedulable.cause.claimUnbound": (p: P) =>
    `${p.service ?? ""} 无法启动：它的卷还没准备好。`,
  "sentence.diagnosis.unschedulable.whyClaim": (p: P) =>
    `副本要等卷准备好才能启动。集群的说明：${p.message ?? ""}`,
  "sentence.diagnosis.unschedulable.fix.claimMissing": (p: P) =>
    `创建名为 ${p.claim ?? ""} 的卷，或让服务改用现有的卷。`,
  "sentence.diagnosis.unschedulable.fix.claimUnbound": () =>
    "卷的事件会说明为什么还没有存储空间，常见原因是集群没有默认的存储类。",
  "sentence.diagnosis.unschedulable.fix.cpu": () => "减少预留的 CPU、减少副本数，或加一台机器。",
  "sentence.diagnosis.unschedulable.fix.memory": () => "减少预留的内存、减少副本数，或加一台机器。",
  "sentence.diagnosis.unschedulable.fix.placement": () =>
    "检查它可以在哪里运行（node selector、affinity、tolerations）。",
  "sentence.diagnosis.unschedulable.fix.other": () => "副本的事件里有调度器完整的响应。",
  "sentence.diagnosis.quotaBlocked.cause": (p: P) =>
    `${p.service ?? ""} 无法再启动更多副本：已达到项目用量上限 ${p.quota ?? ""}。`,
  "sentence.diagnosis.quotaBlocked.why": (p: P) =>
    `项目用量上限 ${p.quota ?? ""} 限制所有服务加起来能预留的量，超过时集群会拒绝新的副本。`,
  "sentence.diagnosis.quotaBlocked.fix": () =>
    "减少副本数、调低每个副本的预留量，或请管理员调高上限。",
  "sentence.diagnosis.podSecurity.cause.runsAsRoot": (p: P) =>
    `${p.container ?? ""} 无法启动：镜像以 root 运行，但服务的安全性设为“严格”。`,
  "sentence.diagnosis.podSecurity.cause.nonNumericUser": (p: P) =>
    `${p.container ?? ""} 无法启动：服务的安全性设为“严格”，但镜像用名称指定用户，集群无法确认它不是 root。`,
  "sentence.diagnosis.podSecurity.cause.refused": (p: P) =>
    `${p.service ?? ""} 无法启动副本：不符合项目的 Pod Security 等级 ${p.level ?? ""}。`,
  "sentence.diagnosis.podSecurity.why.runAsNonRoot": () =>
    "“严格”安全性要求集群不以 root 启动容器。集群会检查镜像运行的用户，遇到 root 或无法检查的用户就拒绝启动。",
  "sentence.diagnosis.podSecurity.why.refused": (p: P) =>
    `集群会拒绝不符合项目 ${p.level ?? ""} 等级的副本：${p.breaks ?? ""}。`,
  "sentence.diagnosis.podSecurity.fix.compatible": () =>
    "在服务设置里把安全性改为“兼容”。想保持“严格”，就改用不需要 root 的镜像，例如用 nginxinc/nginx-unprivileged 替代 nginx，或在 Dockerfile 里用数字指定用户，例如 USER 1000。",
  "sentence.diagnosis.podSecurity.fix.numericUser": () =>
    "在镜像里用数字指定用户（USER 101，而不是 USER nginx），或在服务设置里把安全性改为“兼容”。",
  "sentence.diagnosis.podSecurity.fix.strict": () =>
    "这个项目只允许“严格”的服务。在服务设置里把安全性改为“严格”，或请管理员把项目的 Pod Security 等级改为 baseline。",
  "sentence.diagnosis.podSecurity.fix.privileged": (p: P) =>
    `这个服务有些设置不符合项目 ${p.level ?? ""} 等级。移除这些设置，或请管理员放宽。`,
  "sentence.diagnosis.unreachable.cause.noService": (p: P) =>
    `集群里的其他服务还连不到 ${p.service ?? ""}：它没有私有地址（Service）。`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p: P) =>
    `${p.endpoint ?? ""} 把流量送到端口 ${p.targetPort ?? ""}，但 ${p.service ?? ""} 的容器没有声明这个端口${!p.ports || p.ports === "none" ? `` : `，它声明的是 ${p.ports ?? ""}`}。`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p: P) =>
    `${!p.endpoint || p.endpoint === "none" ? `${p.address ?? ""}` : `${p.endpoint ?? ""}`} 没有地方可以送流量：${p.service ?? ""} 没有任何就绪的副本。`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p: P) =>
    `发往 ${p.route ?? ""} 域名的请求到不了 ${p.service ?? ""}：流量入口报告 ${p.condition ?? ""} ${p.reason ?? ""}。`,
  "sentence.diagnosis.unreachable.why.noService": (p: P) =>
    `${!p.from || p.from === "none" ? `路由用名称 ${!p.endpoint || p.endpoint === "none" ? `${p.service ?? ""}` : `${p.endpoint ?? ""}`} 发送流量，但要有同名的私有地址（Service）才收得到。` : `${p.from ?? ""} 用名称 ${p.service ?? ""} 发送流量给它，但要有同名的私有地址（Service）才收得到。`}`,
  "sentence.diagnosis.unreachable.why.targetPort": () =>
    "私有地址会转发到它指定的端口。用名称指定时，必须是容器声明的端口。",
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
    "检查路由的流量入口和它指向的地址，路由的状态会说是哪一个。",
  "sentence.diagnosis.unexplained.cause.waiting": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 正在等待（${p.reason ?? ""}）。` : `${p.pod ?? ""} 没有在运行：${p.container ?? ""} 正在等待（${p.reason ?? ""}）。`}`,
  "sentence.diagnosis.unexplained.cause.terminated": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 已停止（${p.reason ?? ""}）。` : `${p.pod ?? ""} 没有在运行：${p.container ?? ""} 已停止（${p.reason ?? ""}）。`}`,
  "sentence.diagnosis.unexplained.cause.pending": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 还没启动（Pending）。` : `${p.pod ?? ""} 还没启动（Pending）。`}`,
  "sentence.diagnosis.unexplained.cause.notReady": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} ${!p.check || p.check === "none" ? `在运行，但还没就绪` : `在运行，但没有通过就绪检查（${p.check ?? ""}）`}。` : `${p.pod ?? ""} ${!p.check || p.check === "none" ? `在运行，但还没就绪` : `在运行，但没有通过就绪检查（${p.check ?? ""}）`}。`}`,
  "sentence.diagnosis.unexplained.cause.phase": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 的状态是 ${p.phase ?? ""}。` : `${p.pod ?? ""} 的状态是 ${p.phase ?? ""}。`}`,
  "sentence.diagnosis.unexplained.cause.none": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了。` : `${!p.reason || p.reason === "none" ? `${p.service ?? ""} 异常。` : `${p.service ?? ""} 异常（${p.reason ?? ""}）。`}`}`,
  "sentence.diagnosis.unexplained.why.event": (p: P) =>
    `关于它的最新事件：${p.event ?? ""}，“${p.note ?? ""}”。${!p.serving || p.serving === "none" ? `` : p.serving === "0" ? `` : `上一个版本的 ${p.serving ?? ""} 个副本仍在提供服务。`}`,
  "sentence.diagnosis.unexplained.why.noEvent": (p: P) =>
    `集群没有关于它的事件。${!p.serving || p.serving === "none" ? `` : p.serving === "0" ? `` : `上一个版本的 ${p.serving ?? ""} 个副本仍在提供服务。`}`,
  "sentence.diagnosis.unexplained.fix.waiting": (p: P) =>
    `打开 ${p.pod ?? ""} 的事件，kubelet 会在那里说明它在等什么。`,
  "sentence.diagnosis.unexplained.fix.terminated": (p: P) =>
    `从 ${p.pod ?? ""} 的日志和事件找出它停止的原因。`,
  "sentence.diagnosis.unexplained.fix.pending": (p: P) =>
    `打开 ${p.pod ?? ""} 的事件，调度器和 kubelet 会在那里说明它为什么还没启动。`,
  "sentence.diagnosis.unexplained.fix.notReady": (p: P) =>
    `确认应用在就绪检查查看的位置有响应，再看 ${p.pod ?? ""} 的日志。`,
  "sentence.diagnosis.unexplained.fix.phase": (p: P) => `查看 ${p.pod ?? ""} 的事件和日志。`,
  "sentence.diagnosis.unexplained.fix.none": (p: P) => `查看 ${p.service ?? ""} 的事件。`,
  "sentence.diagnosis.crashLoop.cause.exitCode": (p: P) => `${p.container ?? ""} 反复崩溃、重启。`,
  "sentence.diagnosis.crashLoop.cause.exitsZero": (p: P) =>
    `${p.container ?? ""} 反复正常退出后又重启。`,
  "sentence.diagnosis.crashLoop.cause.notExecutable": (p: P) =>
    `${p.container ?? ""} 无法运行它的命令：这个文件不能执行。`,
  "sentence.diagnosis.crashLoop.cause.commandNotFound": (p: P) =>
    `${p.container ?? ""} 无法运行它的命令：镜像里没有这个命令。`,
  "sentence.diagnosis.crashLoop.cause.killed": (p: P) =>
    `${p.container ?? ""} 反复被强制终止、重启。`,
  "sentence.diagnosis.crashLoop.cause.terminated": (p: P) =>
    `${p.container ?? ""} 反复收到停止请求后重启。`,
  "sentence.diagnosis.crashLoop.fix.exitsZero": () =>
    "服务必须持续运行。只运行一次的程序改用批处理任务（Job）运行。",
  "sentence.diagnosis.crashLoop.fix.command": () => "检查镜像的 entrypoint、command 与 args。",
  "sentence.diagnosis.crashLoop.fix.killed": () => "它是被强制终止的：检查内存用量与存活检查。",
  "sentence.diagnosis.crashLoop.fix.terminated": () => "检查存活检查，以及是什么让程序停止。",
  "sentence.diagnosis.crashLoop.fix.logs": () => "错误信息在重启前那一次的日志里。",
  "sentence.diagnosis.runFailed.fix.logs": () => "错误信息在失败那次运行的日志里。",
  "sentence.diagnosis.runFailed.cause.exitCode": (p: P) => `${p.container ?? ""} 上一次运行失败。`,
  "sentence.diagnosis.runFailed.cause.exitsZero": (p: P) =>
    `${p.container ?? ""} 上一次运行意外结束。`,
  "sentence.diagnosis.runFailed.cause.notExecutable": (p: P) =>
    `${p.container ?? ""} 上一次运行无法启动命令：这个文件不能执行。`,
  "sentence.diagnosis.runFailed.cause.commandNotFound": (p: P) =>
    `${p.container ?? ""} 上一次运行在镜像里找不到命令。`,
  "sentence.diagnosis.runFailed.cause.killed": (p: P) =>
    `${p.container ?? ""} 上一次运行被强制终止。`,
  "sentence.diagnosis.runFailed.cause.terminated": (p: P) =>
    `${p.container ?? ""} 上一次运行收到停止请求而结束。`,
  "sentence.diagnosis.outOfMemory.cause.limit": (p: P) =>
    `${p.container ?? ""} 用完了内存，系统终止了它。`,
  "sentence.diagnosis.outOfMemory.cause.noLimit": (p: P) =>
    `机器的内存不足，系统终止了 ${p.container ?? ""}。`,
  "sentence.diagnosis.outOfMemory.fix.limit": () => "调高内存上限，或找出用掉内存的地方。",
  "sentence.diagnosis.outOfMemory.fix.noLimit": () => "按实际用量设置内存的预留量与上限。",
  "sentence.diagnosis.imagePull.cause.notFound": (p: P) =>
    `${p.container ?? ""} 无法下载镜像 ${p.image ?? ""}：镜像仓库里没有这个镜像。`,
  "sentence.diagnosis.imagePull.cause.unauthorized": (p: P) =>
    `${p.container ?? ""} 无法下载镜像：镜像仓库拒绝了登录信息。`,
  "sentence.diagnosis.imagePull.cause.rateLimited": (p: P) =>
    `${p.container ?? ""} 无法下载镜像：超出了镜像仓库的下载次数限制。`,
  "sentence.diagnosis.imagePull.cause.unreachable": (p: P) =>
    `${p.container ?? ""} 无法下载镜像：连不上镜像仓库。`,
  "sentence.diagnosis.imagePull.cause.invalidName": (p: P) =>
    `${p.container ?? ""} 的镜像名称 ${p.image ?? ""} 有误。`,
  "sentence.diagnosis.imagePull.cause.unknown": (p: P) =>
    `${p.container ?? ""} 无法下载镜像 ${p.image ?? ""}。`,
  "sentence.diagnosis.imagePull.fix.notFound": () => "确认镜像的名称与版本标签。",
  "sentence.diagnosis.imagePull.fix.unauthorized": () =>
    "为项目加上镜像仓库的登录信息（imagePullSecret），或修正现有的。",
  "sentence.diagnosis.imagePull.fix.rateLimited": () => "登录镜像仓库，或改用镜像站。",
  "sentence.diagnosis.imagePull.fix.unreachable": () => "检查机器的网络与 DNS。",
  "sentence.diagnosis.imagePull.fix.invalidName": () => "修正镜像名称。",
  "sentence.diagnosis.imagePull.fix.unknown": () => "副本的事件里有镜像仓库的响应。",
  "sentence.diagnosis.probeFailure.cause.liveness": (p: P) =>
    `${p.container ?? ""} 的存活检查失败 ${p.count ?? ""} 次${!p.output || p.output === "none" ? `。` : `：${p.output ?? ""}`}`,
  "sentence.diagnosis.probeFailure.cause.readiness": (p: P) =>
    `${p.container ?? ""} 的就绪检查失败 ${p.count ?? ""} 次${!p.output || p.output === "none" ? `。` : `：${p.output ?? ""}`}`,
  "sentence.diagnosis.probeFailure.cause.startup": (p: P) =>
    `${p.container ?? ""} 没有在时限内完成启动。`,
  "sentence.diagnosis.probeFailure.fix.liveness": () =>
    "到健康检查确认存活检查的路径、端口与时间设置。",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "到健康检查确认就绪检查的路径、端口与时间设置。",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "如果只是启动得比较慢，到健康检查放宽时间。如果早该启动好了，日志会说明原因。",
  "sentence.gap.resourceLimits.none": (p: P) =>
    `${p.container ?? ""} 没有设置 CPU 与内存的 request 或 limit。`,
  "sentence.gap.resourceLimits.partial": (p: P) =>
    `${p.container ?? ""} 没有设置 ${words(items(p.missing))}。`,
  "sentence.gap.healthChecks.none": (p: P) =>
    `${p.container ?? ""} 没有就绪检查，未就绪时也可能收到流量。`,
  "sentence.gap.healthChecks.livenessOnly": (p: P) =>
    `${p.container ?? ""} 有存活检查，但没有就绪检查。`,
  "sentence.gap.ingressToHttpRoute.retired": (p: P) =>
    `Ingress ${p.ingress ?? ""} 使用 ${p.class ?? ""}，它的 controller 已停止维护。`,
  "sentence.gap.projectWithoutNetworkPolicy": (p: P) =>
    `项目 ${p.namespace ?? ""} 没有 NetworkPolicy ${p.policy ?? ""}，集群里任何副本都连得到它的服务。`,
  "sentence.gap.retiredIngressController": (p: P) =>
    `IngressClass ${p.class ?? ""} 的 controller ${p.controller ?? ""} 已停止维护。`,
  "sentence.gap.danglingRoute.noService": (p: P) =>
    `${p.kind ?? ""} ${p.route ?? ""} 指定的 Service ${words(items(p.services))} 不在这个项目里。`,
  "sentence.gap.danglingRoute.noGateway": (p: P) =>
    `HTTPRoute ${p.route ?? ""} 指定的 Gateway ${words(items(p.gateways))} 不存在。`,
  "sentence.gap.danglingRoute.noParent": (p: P) =>
    `HTTPRoute ${p.route ?? ""} 没有指定 Gateway，所以它收不到任何流量。`,
  "sentence.gap.danglingRoute.notAccepted": (p: P) =>
    `没有任何 Gateway 接受 HTTPRoute ${p.route ?? ""}${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}，所以它收不到任何流量。`,
  "sentence.gap.blockedConnection": (p: P) =>
    `${p.at === "egress" ? `${p.from ?? ""} 的环境变量指向 ${p.to ?? ""}，但 ${p.from ?? ""} 的 NetworkPolicy 不允许连出。` : p.at === "both" ? `${p.from ?? ""} 的环境变量指向 ${p.to ?? ""}，但两端的 NetworkPolicy 都拦截了这条连接。` : `${p.from ?? ""} 的环境变量指向 ${p.to ?? ""}，但 ${p.to ?? ""} 的 NetworkPolicy 不允许 ${p.from ?? ""} 连入。`}`,
  "sentence.gap.noNetworkPolicy": (p: P) =>
    `没有 NetworkPolicy 保护 ${words(items(p.services))}：集群里任何副本都连得到。NetworkPolicy 要集群的网络插件（CNI）支持才会生效。${!p.cuts || p.cuts === "none" ? `` : `隔离后也会拦截${items(p.cuts)[0] === "published" ? ` ` : ``}${words(items(p.cuts).flatMap((item) => (item === "otherNamespaces" ? [`来自其他 Namespace 的连接`] : item === "outside" ? [`连到集群外的连接`] : item === "published" ? [`NodePort 与 LoadBalancer 的流量`] : item === "connections" ? [`服务之间的一条连接`] : [])))}。`}`,
  "sentence.gap.volumeWithoutBackup": (p: P) => `卷 ${p.claim ?? ""} 没有定时备份。`,
  "sentence.gap.databaseWithoutBackup": (p: P) => `数据库 ${p.database ?? ""} 没有定时备份。`,
  "sentence.incident.componentDown.cause.ingressController": (p: P) =>
    `Ingress 控制器 ${p.workload ?? ""} 停摆了：它没有任何就绪的副本。`,
  "sentence.incident.componentDown.cause.certManager": (p: P) =>
    `cert-manager 停摆了：${p.workload ?? ""} 没有任何就绪的副本。`,
  "sentence.incident.componentDown.cause.clusterDns": (p: P) =>
    `集群 DNS 停摆了：${p.workload ?? ""} 没有任何就绪的副本。`,
  "sentence.incident.componentDown.cause.metricsServer": (p: P) =>
    `metrics-server 停摆了：${p.workload ?? ""} 没有任何就绪的副本。`,
  "sentence.incident.componentDown.why.ingressController": (p: P) =>
    `Ingress 控制器负责把公开域名的请求送到对应的服务。它停摆时，${p.domains ?? ""} 个域名和 ${p.services ?? ""} 个服务收不到集群外部的请求。`,
  "sentence.incident.componentDown.why.certManager": (p: P) =>
    `cert-manager 负责签发和续期域名要求的证书。它停摆时，${p.domains ?? ""} 个域名的证书不会签发也不会续期，证书一过期，浏览器就会拒绝。`,
  "sentence.incident.componentDown.why.clusterDns": (p: P) =>
    `服务之间靠集群 DNS 用名称找到彼此。它停摆时，有 ${p.services ?? ""} 个服务用名称连接，会连不上对方。`,
  "sentence.incident.componentDown.why.metricsServer": () =>
    "metrics-server 负责测量每个副本的用量。它停摆时，看不到用量数据，自动扩缩容也会停止。",
  "sentence.incident.componentDown.fix.ingressController": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}没有就绪的副本，从它的事件和日志找出原因。`,
  "sentence.incident.componentDown.fix.certManager": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}没有就绪的副本，从它的事件和日志找出原因。`,
  "sentence.incident.componentDown.fix.clusterDns": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}没有就绪的副本，从它的事件和日志找出原因。`,
  "sentence.incident.componentDown.fix.metricsServer": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}没有就绪的副本，从它的事件和日志找出原因。`,
  "sentence.incident.nodeNotReady.cause": (p: P) =>
    `${p.status === "Unknown" ? `节点 ${p.node ?? ""} 不再向集群报告状态` : `节点 ${p.node ?? ""} 未就绪`}${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}。`,
  "sentence.incident.nodeNotReady.why": (p: P) =>
    `节点未就绪时不会再启动新的副本。宽限期（默认 5 分钟）过后，集群会移除上面的副本，让它们到别处启动。这个节点上有 ${p.pods ?? ""} 个副本，${p.services ?? ""} 个服务有副本在上面。`,
  "sentence.incident.nodeNotReady.fix": (p: P) =>
    `检查 ${p.node ?? ""} 背后的机器：是否开机、网络，以及节点代理（kubelet）。如果它无法恢复，就清空这个节点并替换它。`,
  "sentence.trace.dns.ok": (p: P) => `${p.host ?? ""} 可以解析。`,
  "sentence.trace.dns.notFound": (p: P) =>
    `从集群内解析不到 ${p.host ?? ""}。集群外的 DNS 可能查得到，所以 Zeabur 无法确定。`,
  "sentence.trace.dns.unknown": (p: P) => `没有查询 ${p.host ?? ""}，或查询没有及时响应。`,
  "sentence.trace.address.noStatus": (p: P) =>
    `${p.route ?? ""} 的状态里还没有地址，无法和域名的地址比对。`,
  "sentence.trace.address.unresolved": (p: P) =>
    `域名没有解析出地址，无法和 ${p.route ?? ""} 的地址比对。`,
  "sentence.trace.address.ok": (p: P) => `域名指向 ${p.route ?? ""} 提供服务的地址。`,
  "sentence.trace.address.elsewhere": (p: P) =>
    `域名指向的地址不是 ${p.route ?? ""} 的。如果集群前面有 CDN 或代理服务器，响应的会是它们的地址。`,
  "sentence.trace.controller.classesUnreadable": (p: P) =>
    `Zeabur 无法读取集群的 IngressClass，因此无法判断哪个控制器负责 ${p.route ?? ""}。`,
  "sentence.trace.controller.classMissing": (p: P) =>
    `${p.route ?? ""} 指定的 IngressClass ${p.class ?? ""} 不在集群里。`,
  "sentence.trace.controller.unreadable": (p: P) =>
    `Zeabur 无法读取集群的组件，因此无法判断 ${p.route ?? ""} 的 Ingress 控制器是否在运行。`,
  "sentence.trace.controller.notRecognised": (p: P) =>
    `Zeabur 识别不出负责 ${p.route ?? ""} 的 Ingress 控制器${!p.controller || p.controller === "none" ? `` : ` ${p.controller ?? ""}`}，因此无法判断它是否在运行。`,
  "sentence.trace.controller.ok": (p: P) => `Ingress 控制器 ${p.workload ?? ""} 有就绪的副本。`,
  "sentence.trace.controller.down": (p: P) =>
    `Ingress 控制器 ${p.workload ?? ""} 没有就绪的副本，所以没有东西处理 ${p.route ?? ""}。`,
  "sentence.trace.certificate.defaultCertificate": (p: P) =>
    `${p.route ?? ""} 为 ${p.host ?? ""} 启用 HTTPS，却没有指定机密变量，所以控制器会提供它的默认证书。`,
  "sentence.trace.certificate.ok": (p: P) =>
    `${p.host ?? ""} 的证书已签发${!p.until || p.until === "none" ? `` : `，有效期至 ${p.until ?? ""}`}。`,
  "sentence.trace.certificate.notReady": (p: P) =>
    `${p.host ?? ""} 的证书 ${p.certificate ?? ""} 尚未签发${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}。`,
  "sentence.trace.certificate.issuing": (p: P) =>
    `cert-manager 正在为 ${p.host ?? ""} 签发证书 ${p.certificate ?? ""}。`,
  "sentence.trace.certificate.pending": (p: P) =>
    `${p.host ?? ""} 的证书 ${p.certificate ?? ""} 还没有结果。`,
  "sentence.trace.certificate.unreadable": (p: P) =>
    `Zeabur 无法读取项目的机密变量，因此无法检查机密变量 ${p.secret ?? ""} 里 ${p.host ?? ""} 的证书。`,
  "sentence.trace.certificate.notChecked": (p: P) =>
    `机密变量 ${p.secret ?? ""} 存有 ${p.host ?? ""} 的证书，Zeabur 不会进一步检查。`,
  "sentence.trace.certificate.secretMissing": (p: P) =>
    `${p.route ?? ""} 从机密变量 ${p.secret ?? ""} 读取 ${p.host ?? ""} 的证书，但项目里没有这个机密变量。控制器可能改用默认证书，Zeabur 无法检查它。`,
  "sentence.trace.route.noStatus": (p: P) =>
    `还没有任何组件写入 ${p.route ?? ""} 的状态，无法得知它是否已生效。`,
  "sentence.trace.route.ok": (p: P) => `${p.route ?? ""} 已生效：控制器已在它的状态里写入地址。`,
  "sentence.trace.route.accepted": (p: P) => `Gateway 已接受 ${p.route ?? ""}。`,
  "sentence.trace.route.notAccepted": (p: P) =>
    `${p.route ?? ""} 的流量入口报告 ${p.condition ?? ""} ${p.reason ?? ""}，请求无法通过。`,
  "sentence.trace.service.unreadable": (p: P) =>
    `Zeabur 无法读取项目的私有地址（Service），因此无法检查 ${p.endpoint ?? ""}。`,
  "sentence.trace.service.missing": (p: P) =>
    `找不到名为 ${p.endpoint ?? ""} 的私有地址（Service）。`,
  "sentence.trace.service.portMissing": (p: P) =>
    `私有地址 ${p.endpoint ?? ""} 没有端口 ${p.port ?? ""}。`,
  "sentence.trace.service.selectsOther": (p: P) =>
    `私有地址 ${p.endpoint ?? ""} 把流量发到其他副本，而不是 ${p.service ?? ""} 的副本。`,
  "sentence.trace.service.ok": (p: P) =>
    `私有地址 ${p.endpoint ?? ""} 把流量发到 ${p.service ?? ""}。`,
  "sentence.trace.targetPort.unknown": (p: P) =>
    `Zeabur 读不到容器，因此没有检查 ${p.endpoint ?? ""} 转发到的端口。`,
  "sentence.trace.targetPort.notReached": (p: P) =>
    `没有检查：流量在到达 ${p.endpoint ?? ""} 的端口之前就停了。`,
  "sentence.trace.targetPort.mismatch": (p: P) =>
    `${p.endpoint ?? ""} 把流量送到端口 ${p.targetPort ?? ""}，但容器没有声明这个端口${!p.ports || p.ports === "none" ? `` : `，它声明的是 ${p.ports ?? ""}`}。`,
  "sentence.trace.targetPort.undeclared": (p: P) =>
    `容器没有声明任何端口，无法得知它们是否在 ${p.endpoint ?? ""} 转发到的端口上监听。`,
  "sentence.trace.targetPort.ok": (p: P) => `${p.endpoint ?? ""} 把流量送到容器声明的端口。`,
  "sentence.trace.ready.scaledToZero": (p: P) => `${p.service ?? ""} 的副本数设为 0。`,
  "sentence.trace.ready.ok": (p: P) =>
    `${p.service ?? ""} 有 ${p.ready ?? ""}/${p.pods ?? ""} 个副本就绪。`,
  "sentence.trace.ready.starting": (p: P) => `${p.service ?? ""} 正在启动，还没有就绪的副本。`,
  "sentence.trace.ready.noneReady": (p: P) => `${p.service ?? ""} 没有任何就绪的副本。`,
  "sentence.diagnosis.unreachable.cause.ingressClassMissing": (p: P) =>
    `发往 ${p.route ?? ""} 域名的请求到不了 ${p.service ?? ""}：它指定的 IngressClass ${p.class ?? ""} 不在集群里。`,
  "sentence.diagnosis.unreachable.why.ingressClassMissing": () =>
    "Ingress 控制器只处理自己类别的 Ingress，没有这个类别，就没有控制器处理它。",
  "sentence.diagnosis.unreachable.fix.ingressClassMissing": (p: P) =>
    `把 ${p.route ?? ""} 的类别改成集群里有的类别，或安装 ${p.class ?? ""} 的控制器。`,
  "sentence.diagnosis.unreachable.cause.controllerDown": (p: P) =>
    `发往 ${p.route ?? ""} 域名的请求到不了 ${p.service ?? ""}：Ingress 控制器 ${p.workload ?? ""} 没有就绪的副本。`,
  "sentence.diagnosis.unreachable.why.controllerDown": () =>
    "Ingress 控制器负责把域名的请求带进集群。它没有任何就绪的副本时，这些请求就得不到响应。",
  "sentence.diagnosis.unreachable.fix.controllerDown": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}没有就绪的副本，从它的事件和日志找出原因。`,
  "sentence.diagnosis.unreachable.cause.certificateNotReady": (p: P) =>
    `${p.host ?? ""} 的 HTTPS 连接失败：证书 ${p.certificate ?? ""} 尚未签发${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}。`,
  "sentence.diagnosis.unreachable.why.certificateNotReady": (p: P) =>
    `cert-manager 签发证书后才会把它写进机密变量 ${p.secret ?? ""}。在那之前，控制器会改用默认证书，浏览器会拒绝它。`,
  "sentence.diagnosis.unreachable.fix.certificateNotReady": (p: P) =>
    `证书 ${p.certificate ?? ""} 的状态和事件会说明它为什么还没签发。`,
  "sentence.diagnosis.unreachable.cause.backendPortMissing": (p: P) =>
    `通往 ${p.service ?? ""} 的路由指定了端口 ${p.port ?? ""}，但它的私有地址 ${p.endpoint ?? ""} 没有这个端口。`,
  "sentence.diagnosis.unreachable.why.backendPortMissing": () =>
    "路由会用编号或名称把流量发到私有地址的某个端口。私有地址没有的端口收不到任何流量。",
  "sentence.diagnosis.unreachable.fix.backendPortMissing": (p: P) =>
    `让路由指向 ${p.endpoint ?? ""} 有的端口，或在 ${p.endpoint ?? ""} 加上端口 ${p.port ?? ""}。`,
  "sentence.diagnosis.unreachable.cause.selectsOther": (p: P) =>
    `通往 ${p.service ?? ""} 的路由把流量发到 ${p.endpoint ?? ""}，但 ${p.endpoint ?? ""} 会发到其他副本，而不是 ${p.service ?? ""} 的副本。`,
  "sentence.diagnosis.unreachable.why.selectsOther": () =>
    "私有地址只会把流量发到标签符合它 selector 的副本。",
  "sentence.diagnosis.unreachable.fix.selectsOther": (p: P) =>
    `让 ${p.endpoint ?? ""} 的 selector 匹配 ${p.service ?? ""} 的标签，或让路由改为指向 ${p.service ?? ""} 自己的私有地址。`,
  "sentence.diagnosis.unreachable.cause.scaledToZero": (p: P) =>
    `没有流量到得了 ${p.service ?? ""}：它的副本数设为 0。`,
  "sentence.diagnosis.unreachable.why.scaledToZero": () =>
    "私有地址只会把流量发到运行中的副本，而目前配置为不运行任何副本。",
  "sentence.diagnosis.unreachable.fix.scaledToZero": (p: P) =>
    `把 ${p.service ?? ""} 的副本数调到至少 1。`,
  "sentence.gap.ephemeralData.none": (p: P) =>
    `${p.container ?? ""} 把 ${p.software ?? ""} 数据存在 ${p.path ?? ""}，但这个路径不在任何卷上，副本一替换，数据就没了。`,
  "sentence.gap.ephemeralData.ephemeral": (p: P) =>
    `${p.container ?? ""} 把 ${p.software ?? ""} 数据存在 ${p.path ?? ""}，但这个卷只随副本存在，副本一替换，数据就没了。`,
}
