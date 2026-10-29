import { durationOf, items, joinWords, type P } from "@/i18n/sentence-parts"

const seconds = (n: number) => `${n} 秒`
const minutes = (n: number) => `${n} 分鐘`
const words = (list: string[]) => joinWords(list, "、", "、")
const duration = (value: string | undefined) => durationOf(value, seconds, minutes)

export const serverZhTw = {
  "sentence.diagnosis.crashLoop.why.backOff": (p: P) =>
    `容器已重新啟動 ${p.restarts ?? ""} 次。叢集每次都會再次啟動，兩次之間等得越來越久，預設最多等 5 分鐘。`,
  "sentence.diagnosis.crashLoop.why.exitsZero": (p: P) =>
    `服務應該持續執行，所以程式一結束，即使是正常結束，叢集也會再次啟動（目前 ${p.restarts ?? ""} 次）。`,
  "sentence.diagnosis.runFailed.why.exitCode": (p: P) =>
    `程式的結束碼不是 0，就算執行失敗。這次的結束碼是 ${p.exitCode ?? ""}。`,
  "sentence.diagnosis.runFailed.why.gaveUp": (p: P) =>
    `${!p.backoffLimit || p.backoffLimit === "none" ? `這項工作重試後已放棄，最後一次的結束碼是 ${p.exitCode ?? ""}。` : `${p.backoffLimit === "0" ? `這項工作失敗後不會重試，這次的結束碼是 ${p.exitCode ?? ""}。` : `這項工作已放棄：執行失敗時會重試 ${p.backoffLimit ?? ""} 次，最後一次的結束碼也是 ${p.exitCode ?? ""}。`}`}`,
  "sentence.diagnosis.outOfMemory.why.limit": (p: P) =>
    `每個副本最多只能用 ${p.limit ?? ""} 記憶體，用到上限時系統會直接終止程式，程式來不及處理。`,
  "sentence.diagnosis.outOfMemory.why.noLimit": () =>
    "沒有記憶體上限時，程式可以一直用到機器的記憶體不足，這時系統會終止用最多的程式。",
  "sentence.diagnosis.imagePull.why": () =>
    "叢集要先下載映像檔才能啟動容器。下載失敗時會一再重試，每次等得更久。",
  "sentence.diagnosis.probeFailure.why.liveness": (p: P) =>
    `${!p.period || p.period === "none" ? `存活檢查連續失敗幾次後，叢集就會重新啟動容器。` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `預設情況下，叢集每 ${duration(p.period)}檢查一次，連續失敗 ${p.threshold ?? ""} 次（約 ${duration(p.total)}）就會重新啟動容器。` : `叢集每 ${duration(p.period)}檢查一次，連續失敗 ${p.threshold ?? ""} 次（約 ${duration(p.total)}）就會重新啟動容器。每 ${duration(p.period)}是預設值。`}` : `叢集每 ${duration(p.period)}檢查一次，連續失敗 ${p.threshold ?? ""} 次（約 ${duration(p.total)}）就會重新啟動容器。${p.thresholdDefault === "true" ? `${p.threshold ?? ""} 次是預設值。` : ``}`}`}`,
  "sentence.diagnosis.probeFailure.why.readiness": (p: P) =>
    `${!p.period || p.period === "none" ? `就緒檢查失敗期間，容器不會從私有位址收到流量。` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `預設情況下，叢集每 ${duration(p.period)}檢查一次，連續失敗 ${p.threshold ?? ""} 次（約 ${duration(p.total)}）後，容器在再次通過之前，都不會從私有位址收到流量。` : `叢集每 ${duration(p.period)}檢查一次，連續失敗 ${p.threshold ?? ""} 次（約 ${duration(p.total)}）後，容器在再次通過之前，都不會從私有位址收到流量。每 ${duration(p.period)}是預設值。`}` : `叢集每 ${duration(p.period)}檢查一次，連續失敗 ${p.threshold ?? ""} 次（約 ${duration(p.total)}）後，容器在再次通過之前，都不會從私有位址收到流量。${p.thresholdDefault === "true" ? `${p.threshold ?? ""} 次是預設值。` : ``}`}`}`,
  "sentence.diagnosis.probeFailure.why.startup": (p: P) =>
    `${!p.period || p.period === "none" ? `其他檢查會等啟動檢查通過，如果沒有在時限內通過，叢集就會重新啟動容器。` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `預設情況下，容器有 ${p.threshold ?? ""} × ${duration(p.period)}（${duration(p.total)}）可以通過啟動檢查，超過時叢集就會重新啟動容器。` : `容器有 ${p.threshold ?? ""} × ${duration(p.period)}（${duration(p.total)}）可以通過啟動檢查，超過時叢集就會重新啟動容器。每 ${duration(p.period)}是預設值。`}` : `容器有 ${p.threshold ?? ""} × ${duration(p.period)}（${duration(p.total)}）可以通過啟動檢查，超過時叢集就會重新啟動容器。${p.thresholdDefault === "true" ? `${p.threshold ?? ""} 次是預設值。` : ``}`}`}`,
  "sentence.diagnosis.configMissing.cause.object": (p: P) =>
    `${p.container ?? ""} 無法啟動：${p.object === "Secret" ? `機密變數 ${p.name ?? ""}` : `設定檔 ${p.name ?? ""}`} 不存在。`,
  "sentence.diagnosis.configMissing.cause.key": (p: P) =>
    `${p.container ?? ""} 無法啟動：${p.object === "Secret" ? `機密變數 ${p.name ?? ""}` : `設定檔 ${p.name ?? ""}`} 裡沒有 ${p.key ?? ""}。`,
  "sentence.diagnosis.configMissing.cause.unknown": (p: P) =>
    `${p.container ?? ""} 無法啟動：缺少要讀取的機密變數或設定檔。`,
  "sentence.diagnosis.configMissing.cause.volume": (p: P) =>
    `${!p.container || p.container === "none" ? `副本 ${p.pod ?? ""}` : `${p.container ?? ""}`} 無法啟動：磁碟區 ${p.volume ?? ""} 取自 ${p.object === "Secret" ? `機密變數 ${p.name ?? ""}` : `設定檔 ${p.name ?? ""}`}，但專案裡找不到。`,
  "sentence.diagnosis.configMissing.why": () =>
    "容器讀取的每個機密變數、設定檔和其中的鍵都存在之後，叢集才會啟動容器。",
  "sentence.diagnosis.configMissing.why.volume": () =>
    "叢集會先掛載所有磁碟區，再啟動副本的容器。磁碟區如果取自不存在的機密變數或設定檔，就無法掛載，容器會一直等待。",
  "sentence.diagnosis.configMissing.fix.object": (p: P) =>
    `在專案裡加上 ${p.object === "Secret" ? `機密變數 ${p.name ?? ""}` : `設定檔 ${p.name ?? ""}`}，或讓容器不再讀取。`,
  "sentence.diagnosis.configMissing.fix.key": (p: P) =>
    `在 ${p.object === "Secret" ? `機密變數 ${p.name ?? ""}` : `設定檔 ${p.name ?? ""}`} 加上 ${p.key ?? ""}，或讓容器不再讀取。`,
  "sentence.diagnosis.configMissing.fix.unknown": () =>
    "副本的事件會說明缺少什麼。補上缺少的項目，或讓容器不再讀取。",
  "sentence.diagnosis.configMissing.fix.volume": (p: P) =>
    `在專案裡加上 ${p.object === "Secret" ? `機密變數 ${p.name ?? ""}` : `設定檔 ${p.name ?? ""}`}，或移除磁碟區 ${p.volume ?? ""}。`,
  "sentence.diagnosis.unschedulable.cause.cpu": (p: P) =>
    `${p.service ?? ""} 正在等待可用資源：沒有機器有足夠的 CPU。`,
  "sentence.diagnosis.unschedulable.cause.memory": (p: P) =>
    `${p.service ?? ""} 正在等待可用資源：沒有機器有足夠的記憶體。`,
  "sentence.diagnosis.unschedulable.cause.placement": (p: P) =>
    `${p.service ?? ""} 正在等待允許執行的機器。`,
  "sentence.diagnosis.unschedulable.cause.other": (p: P) =>
    `${p.service ?? ""} 正在等待可以執行的機器。`,
  "sentence.diagnosis.unschedulable.why": (p: P) =>
    `副本保留的資源，機器要容得下，副本才能放上去。叢集的說明：${p.message ?? ""}`,
  "sentence.diagnosis.unschedulable.cause.claimMissing": (p: P) =>
    `${p.service ?? ""} 無法啟動：要掛載的磁碟區 ${p.claim ?? ""} 不存在。`,
  "sentence.diagnosis.unschedulable.cause.claimUnbound": (p: P) =>
    `${p.service ?? ""} 無法啟動：磁碟區還沒準備好。`,
  "sentence.diagnosis.unschedulable.whyClaim": (p: P) =>
    `副本要等磁碟區準備好才能啟動。叢集的說明：${p.message ?? ""}`,
  "sentence.diagnosis.unschedulable.fix.claimMissing": (p: P) =>
    `建立名為 ${p.claim ?? ""} 的磁碟區，或讓服務改用現有的磁碟區。`,
  "sentence.diagnosis.unschedulable.fix.claimUnbound": () =>
    "磁碟區的事件會說明為什麼還沒有儲存空間，常見原因是叢集沒有預設的儲存類別。",
  "sentence.diagnosis.unschedulable.fix.cpu": () => "減少保留的 CPU、減少副本數，或加一台機器。",
  "sentence.diagnosis.unschedulable.fix.memory": () =>
    "減少保留的記憶體、減少副本數，或加一台機器。",
  "sentence.diagnosis.unschedulable.fix.placement": () =>
    "檢查服務能在哪些機器執行（node selector、affinity、tolerations）。",
  "sentence.diagnosis.unschedulable.fix.other": () => "副本事件裡有排程器的完整回應。",
  "sentence.diagnosis.quotaBlocked.cause": (p: P) =>
    `${p.service ?? ""} 無法再啟動更多副本：已達到專案用量上限 ${p.quota ?? ""}。`,
  "sentence.diagnosis.quotaBlocked.why": (p: P) =>
    `專案用量上限 ${p.quota ?? ""} 限制所有服務加起來能保留的量，超過時叢集會拒絕新的副本。`,
  "sentence.diagnosis.quotaBlocked.fix": () =>
    "減少副本數、調低每個副本的保留量，或請管理員調高上限。",
  "sentence.diagnosis.podSecurity.cause.runsAsRoot": (p: P) =>
    `${p.container ?? ""} 無法啟動：映像檔以 root 執行，但服務的安全性設為「嚴格」。`,
  "sentence.diagnosis.podSecurity.cause.nonNumericUser": (p: P) =>
    `${p.container ?? ""} 無法啟動：服務的安全性設為「嚴格」，但映像檔用名稱指定使用者，叢集無法確認那個使用者不是 root。`,
  "sentence.diagnosis.podSecurity.cause.refused": (p: P) =>
    `${p.service ?? ""} 無法啟動副本：不符合專案的 Pod Security 等級 ${p.level ?? ""}。`,
  "sentence.diagnosis.podSecurity.why.runAsNonRoot": () =>
    "「嚴格」安全性要求叢集不以 root 啟動容器。叢集會檢查映像檔執行的使用者，遇到 root 或無法檢查的使用者就拒絕啟動。",
  "sentence.diagnosis.podSecurity.why.refused": (p: P) =>
    `叢集會拒絕不符合專案 ${p.level ?? ""} 等級的副本：${p.breaks ?? ""}。`,
  "sentence.diagnosis.podSecurity.fix.compatible": () =>
    "在服務設定裡把安全性改為「相容」。想維持「嚴格」，就改用不需要 root 的映像檔，例如用 nginxinc/nginx-unprivileged 取代 nginx，或在 Dockerfile 裡用數字指定使用者，例如 USER 1000。",
  "sentence.diagnosis.podSecurity.fix.numericUser": () =>
    "在映像檔裡用數字指定使用者（USER 101，而不是 USER nginx），或在服務設定裡把安全性改為「相容」。",
  "sentence.diagnosis.podSecurity.fix.strict": () =>
    "這個專案只允許「嚴格」的服務。在服務設定裡把安全性改為「嚴格」，或請管理員把專案的 Pod Security 等級改為 baseline。",
  "sentence.diagnosis.podSecurity.fix.privileged": (p: P) =>
    `這個服務有些設定不符合專案 ${p.level ?? ""} 等級。移除這些設定，或請管理員放寬。`,
  "sentence.diagnosis.unreachable.cause.noService": (p: P) =>
    `叢集裡的其他服務還連不到 ${p.service ?? ""}：這個服務沒有私有位址（Service）。`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p: P) =>
    `${p.endpoint ?? ""} 把流量送到連接埠 ${p.targetPort ?? ""}，但 ${p.service ?? ""} 的容器沒有宣告這個連接埠${!p.ports || p.ports === "none" ? `` : `，宣告的是 ${p.ports ?? ""}`}。`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p: P) =>
    `${!p.endpoint || p.endpoint === "none" ? `${p.address ?? ""}` : `${p.endpoint ?? ""}`} 沒有地方可以送流量：${p.service ?? ""} 沒有任何就緒的副本。`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p: P) =>
    `送到 ${p.route ?? ""} 網域的請求到不了 ${p.service ?? ""}：流量入口回報 ${p.condition ?? ""} ${p.reason ?? ""}。`,
  "sentence.diagnosis.unreachable.why.noService": (p: P) =>
    `${!p.from || p.from === "none" ? `路由用名稱 ${!p.endpoint || p.endpoint === "none" ? `${p.service ?? ""}` : `${p.endpoint ?? ""}`} 送流量，但要有同名的私有位址（Service）才收得到。` : `${p.from ?? ""} 用名稱 ${p.service ?? ""} 送流量，但要有同名的私有位址（Service）才收得到。`}`,
  "sentence.diagnosis.unreachable.why.targetPort": () =>
    "私有位址會轉送到自己指定的連接埠。用名稱指定時，必須是容器宣告的連接埠。",
  "sentence.diagnosis.unreachable.why.noReadyEndpoints": () =>
    "私有位址只會把流量送給通過就緒檢查的副本。",
  "sentence.diagnosis.unreachable.why.routeNotAccepted": () =>
    "網域的流量入口必須接受這條路由，並找到路由指定的位址，請求才會通過。",
  "sentence.diagnosis.unreachable.fix.noService": () =>
    "新增私有位址（Service），讓其他服務或網域可以把流量送進來。",
  "sentence.diagnosis.unreachable.fix.targetPort": () => "讓私有位址指向容器宣告的連接埠。",
  "sentence.diagnosis.unreachable.fix.noReadyEndpoints": () =>
    "找出為什麼沒有副本就緒：看服務的其他問題、就緒檢查和日誌。",
  "sentence.diagnosis.unreachable.fix.routeNotAccepted": () =>
    "檢查路由用的流量入口，以及路由指向的位址，路由的狀態會說是哪一個。",
  "sentence.diagnosis.unexplained.cause.waiting": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 正在等待（${p.reason ?? ""}）。` : `${p.pod ?? ""} 沒有在執行：${p.container ?? ""} 正在等待（${p.reason ?? ""}）。`}`,
  "sentence.diagnosis.unexplained.cause.terminated": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 已停止（${p.reason ?? ""}）。` : `${p.pod ?? ""} 沒有在執行：${p.container ?? ""} 已停止（${p.reason ?? ""}）。`}`,
  "sentence.diagnosis.unexplained.cause.pending": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 還沒啟動（Pending）。` : `${p.pod ?? ""} 還沒啟動（Pending）。`}`,
  "sentence.diagnosis.unexplained.cause.notReady": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} ${!p.check || p.check === "none" ? `在執行，但還沒就緒` : `在執行，但沒有通過就緒檢查（${p.check ?? ""}）`}。` : `${p.pod ?? ""} ${!p.check || p.check === "none" ? `在執行，但還沒就緒` : `在執行，但沒有通過就緒檢查（${p.check ?? ""}）`}。`}`,
  "sentence.diagnosis.unexplained.cause.phase": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了：新副本 ${p.pod ?? ""} 的狀態是 ${p.phase ?? ""}。` : `${p.pod ?? ""} 的狀態是 ${p.phase ?? ""}。`}`,
  "sentence.diagnosis.unexplained.cause.none": (p: P) =>
    `${p.stuck === "true" ? `${p.service ?? ""} 的部署卡住了。` : `${!p.reason || p.reason === "none" ? `${p.service ?? ""} 異常。` : `${p.service ?? ""} 異常（${p.reason ?? ""}）。`}`}`,
  "sentence.diagnosis.unexplained.why.event": (p: P) =>
    `最新的事件：${p.event ?? ""}，「${p.note ?? ""}」。${!p.serving || p.serving === "none" ? `` : p.serving === "0" ? `` : `上一個版本的 ${p.serving ?? ""} 個副本仍在提供服務。`}`,
  "sentence.diagnosis.unexplained.why.noEvent": (p: P) =>
    `叢集沒有相關的事件。${!p.serving || p.serving === "none" ? `` : p.serving === "0" ? `` : `上一個版本的 ${p.serving ?? ""} 個副本仍在提供服務。`}`,
  "sentence.diagnosis.unexplained.fix.waiting": (p: P) =>
    `開啟 ${p.pod ?? ""} 的事件，kubelet 會在那裡說明在等什麼。`,
  "sentence.diagnosis.unexplained.fix.terminated": (p: P) =>
    `開啟 ${p.pod ?? ""} 的日誌和事件，看為什麼停止。`,
  "sentence.diagnosis.unexplained.fix.pending": (p: P) =>
    `開啟 ${p.pod ?? ""} 的事件，排程器和 kubelet 會在那裡說明為什麼還沒啟動。`,
  "sentence.diagnosis.unexplained.fix.notReady": (p: P) =>
    `確認應用程式回應就緒檢查探測的位置，再看 ${p.pod ?? ""} 的日誌。`,
  "sentence.diagnosis.unexplained.fix.phase": (p: P) => `開啟 ${p.pod ?? ""} 的事件和日誌。`,
  "sentence.diagnosis.unexplained.fix.none": (p: P) => `開啟 ${p.service ?? ""} 的事件。`,
  "sentence.diagnosis.crashLoop.cause.exitCode": (p: P) =>
    `${p.container ?? ""} 反覆崩潰、重新啟動。`,
  "sentence.diagnosis.crashLoop.cause.exitsZero": (p: P) =>
    `${p.container ?? ""} 反覆正常結束後又重新啟動。`,
  "sentence.diagnosis.crashLoop.cause.notExecutable": (p: P) =>
    `${p.container ?? ""} 無法執行指令：這個檔案不能執行。`,
  "sentence.diagnosis.crashLoop.cause.commandNotFound": (p: P) =>
    `${p.container ?? ""} 無法執行指令：映像檔裡沒有這個指令。`,
  "sentence.diagnosis.crashLoop.cause.killed": (p: P) =>
    `${p.container ?? ""} 反覆被強制終止、重新啟動。`,
  "sentence.diagnosis.crashLoop.cause.terminated": (p: P) =>
    `${p.container ?? ""} 反覆收到停止要求後重新啟動。`,
  "sentence.diagnosis.crashLoop.fix.exitsZero": () =>
    "服務必須持續執行。只跑一次的程式改用批次工作（Job）執行。",
  "sentence.diagnosis.crashLoop.fix.command": () => "檢查映像檔的 entrypoint、command 與 args。",
  "sentence.diagnosis.crashLoop.fix.killed": () => "容器是被強制終止的：檢查記憶體用量與存活檢查。",
  "sentence.diagnosis.crashLoop.fix.terminated": () => "檢查存活檢查，以及是什麼讓程式停止。",
  "sentence.diagnosis.crashLoop.fix.logs": () => "錯誤訊息在重新啟動前那一次的日誌裡。",
  "sentence.diagnosis.runFailed.fix.logs": () => "錯誤訊息在失敗那次執行的日誌裡。",
  "sentence.diagnosis.runFailed.cause.exitCode": (p: P) => `${p.container ?? ""} 上一次執行失敗。`,
  "sentence.diagnosis.runFailed.cause.exitsZero": (p: P) =>
    `${p.container ?? ""} 上一次執行意外結束。`,
  "sentence.diagnosis.runFailed.cause.notExecutable": (p: P) =>
    `${p.container ?? ""} 上一次執行無法啟動指令：這個檔案不能執行。`,
  "sentence.diagnosis.runFailed.cause.commandNotFound": (p: P) =>
    `${p.container ?? ""} 上一次執行在映像檔裡找不到指令。`,
  "sentence.diagnosis.runFailed.cause.killed": (p: P) =>
    `${p.container ?? ""} 上一次執行被強制終止。`,
  "sentence.diagnosis.runFailed.cause.terminated": (p: P) =>
    `${p.container ?? ""} 上一次執行收到停止要求而結束。`,
  "sentence.diagnosis.outOfMemory.cause.limit": (p: P) =>
    `${p.container ?? ""} 用完了記憶體，被系統終止了。`,
  "sentence.diagnosis.outOfMemory.cause.noLimit": (p: P) =>
    `機器的記憶體不足，系統終止了 ${p.container ?? ""}。`,
  "sentence.diagnosis.outOfMemory.fix.limit": () => "調高記憶體上限，或找出用掉記憶體的地方。",
  "sentence.diagnosis.outOfMemory.fix.noLimit": () => "依實際用量設定記憶體的保留量與上限。",
  "sentence.diagnosis.imagePull.cause.notFound": (p: P) =>
    `${p.container ?? ""} 無法下載映像檔 ${p.image ?? ""}：映像檔倉庫裡沒有這個映像檔。`,
  "sentence.diagnosis.imagePull.cause.unauthorized": (p: P) =>
    `${p.container ?? ""} 無法下載映像檔：映像檔倉庫拒絕了登入資訊。`,
  "sentence.diagnosis.imagePull.cause.rateLimited": (p: P) =>
    `${p.container ?? ""} 無法下載映像檔：超過映像檔倉庫的下載次數限制。`,
  "sentence.diagnosis.imagePull.cause.unreachable": (p: P) =>
    `${p.container ?? ""} 無法下載映像檔：連不到映像檔倉庫。`,
  "sentence.diagnosis.imagePull.cause.invalidName": (p: P) =>
    `${p.container ?? ""} 的映像檔名稱 ${p.image ?? ""} 有誤。`,
  "sentence.diagnosis.imagePull.cause.unknown": (p: P) =>
    `${p.container ?? ""} 無法下載映像檔 ${p.image ?? ""}。`,
  "sentence.diagnosis.imagePull.fix.notFound": () => "確認映像檔的名稱與版本標籤。",
  "sentence.diagnosis.imagePull.fix.unauthorized": () =>
    "替專案加上映像檔倉庫的登入資訊（imagePullSecret），或修正現有的。",
  "sentence.diagnosis.imagePull.fix.rateLimited": () => "登入映像檔倉庫，或改用鏡像站。",
  "sentence.diagnosis.imagePull.fix.unreachable": () => "檢查機器的網路與 DNS。",
  "sentence.diagnosis.imagePull.fix.invalidName": () => "修正映像檔名稱。",
  "sentence.diagnosis.imagePull.fix.unknown": () => "副本事件裡有映像檔倉庫的回應。",
  "sentence.diagnosis.probeFailure.cause.liveness": (p: P) =>
    `${p.container ?? ""} 的存活檢查失敗 ${p.count ?? ""} 次${!p.output || p.output === "none" ? `。` : `：${p.output ?? ""}`}`,
  "sentence.diagnosis.probeFailure.cause.readiness": (p: P) =>
    `${p.container ?? ""} 的就緒檢查失敗 ${p.count ?? ""} 次${!p.output || p.output === "none" ? `。` : `：${p.output ?? ""}`}`,
  "sentence.diagnosis.probeFailure.cause.startup": (p: P) =>
    `${p.container ?? ""} 沒有在時限內完成啟動。`,
  "sentence.diagnosis.probeFailure.fix.liveness": () =>
    "到健康檢查確認存活檢查的路徑、連接埠與時間設定。",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "到健康檢查確認就緒檢查的路徑、連接埠與時間設定。",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "如果只是啟動得比較慢，到健康檢查放寬時間。如果早該啟動好了，日誌會說明原因。",
  "sentence.gap.resourceLimits.none": (p: P) =>
    `${p.container ?? ""} 沒有設定 CPU 與記憶體的 request 或 limit。`,
  "sentence.gap.resourceLimits.partial": (p: P) =>
    `${p.container ?? ""} 沒有設定 ${words(items(p.missing))}。`,
  "sentence.gap.healthChecks.none": (p: P) =>
    `${p.container ?? ""} 沒有就緒檢查，還沒準備好就可能收到流量。`,
  "sentence.gap.healthChecks.livenessOnly": (p: P) =>
    `${p.container ?? ""} 有存活檢查，但沒有就緒檢查。`,
  "sentence.gap.ingressToHttpRoute.retired": (p: P) =>
    `Ingress ${p.ingress ?? ""} 使用 ${p.class ?? ""}，這個 controller 已停止維護。`,
  "sentence.gap.projectWithoutNetworkPolicy": (p: P) =>
    `專案 ${p.namespace ?? ""} 沒有 NetworkPolicy ${p.policy ?? ""}，叢集裡任何副本都連得到這個專案的服務。`,
  "sentence.gap.retiredIngressController": (p: P) =>
    `IngressClass ${p.class ?? ""} 的 controller ${p.controller ?? ""} 已停止維護。`,
  "sentence.gap.danglingRoute.noService": (p: P) =>
    `${p.kind ?? ""} ${p.route ?? ""} 指定的 Service ${words(items(p.services))} 不在這個專案裡。`,
  "sentence.gap.danglingRoute.noGateway": (p: P) =>
    `HTTPRoute ${p.route ?? ""} 指定的 Gateway ${words(items(p.gateways))} 不存在。`,
  "sentence.gap.danglingRoute.noParent": (p: P) =>
    `HTTPRoute ${p.route ?? ""} 沒有指定 Gateway，所以收不到任何流量。`,
  "sentence.gap.danglingRoute.notAccepted": (p: P) =>
    `沒有任何 Gateway 接受 HTTPRoute ${p.route ?? ""}${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}，所以收不到任何流量。`,
  "sentence.gap.blockedConnection": (p: P) =>
    `${p.at === "egress" ? `${p.from ?? ""} 的環境變數指向 ${p.to ?? ""}，但 ${p.from ?? ""} 的 NetworkPolicy 不允許連出。` : p.at === "both" ? `${p.from ?? ""} 的環境變數指向 ${p.to ?? ""}，但兩端的 NetworkPolicy 都封鎖了這條連線。` : `${p.from ?? ""} 的環境變數指向 ${p.to ?? ""}，但 ${p.to ?? ""} 的 NetworkPolicy 不允許 ${p.from ?? ""} 連入。`}`,
  "sentence.gap.noNetworkPolicy": (p: P) =>
    `沒有 NetworkPolicy 保護 ${words(items(p.services))}：叢集裡任何副本都連得到。NetworkPolicy 要叢集的網路外掛（CNI）支援才會生效。${!p.cuts || p.cuts === "none" ? `` : `隔離後也會封鎖${items(p.cuts)[0] === "published" ? ` ` : ``}${words(items(p.cuts).flatMap((item) => (item === "otherNamespaces" ? [`來自其他 Namespace 的連線`] : item === "outside" ? [`連到叢集外的連線`] : item === "published" ? [`NodePort 與 LoadBalancer 的流量`] : item === "connections" ? [`服務之間的一條連線`] : [])))}。`}`,
  "sentence.gap.volumeWithoutBackup": (p: P) => `磁碟區 ${p.claim ?? ""} 沒有備份排程。`,
  "sentence.gap.databaseWithoutBackup": (p: P) => `資料庫 ${p.database ?? ""} 沒有備份排程。`,
  "sentence.incident.componentDown.cause.ingressController": (p: P) =>
    `Ingress 控制器 ${p.workload ?? ""} 停擺了：沒有任何就緒的副本。`,
  "sentence.incident.componentDown.cause.certManager": (p: P) =>
    `cert-manager 停擺了：${p.workload ?? ""} 沒有任何就緒的副本。`,
  "sentence.incident.componentDown.cause.clusterDns": (p: P) =>
    `叢集 DNS 停擺了：${p.workload ?? ""} 沒有任何就緒的副本。`,
  "sentence.incident.componentDown.cause.metricsServer": (p: P) =>
    `metrics-server 停擺了：${p.workload ?? ""} 沒有任何就緒的副本。`,
  "sentence.incident.componentDown.why.ingressController": (p: P) =>
    `公開網域的請求由 Ingress 控制器轉給服務。控制器停擺時，${p.domains ?? ""} 個網域和 ${p.services ?? ""} 個服務收不到叢集外部的請求。`,
  "sentence.incident.componentDown.why.certManager": (p: P) =>
    `cert-manager 負責簽發和更新網域要求的憑證。cert-manager 停擺時，${p.domains ?? ""} 個網域的憑證不會簽發也不會更新，憑證一過期，瀏覽器就會拒絕。`,
  "sentence.incident.componentDown.why.clusterDns": (p: P) =>
    `服務之間靠叢集 DNS 用名稱找到彼此。DNS 停擺時，有 ${p.services ?? ""} 個服務用名稱連線，會連不到對方。`,
  "sentence.incident.componentDown.why.metricsServer": () =>
    "metrics-server 負責量測每個副本的用量。metrics-server 停擺時，看不到用量數字，自動擴縮也會停止。",
  "sentence.incident.componentDown.fix.ingressController": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}沒有就緒的副本，從事件和日誌找出原因。`,
  "sentence.incident.componentDown.fix.certManager": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}沒有就緒的副本，從事件和日誌找出原因。`,
  "sentence.incident.componentDown.fix.clusterDns": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}沒有就緒的副本，從事件和日誌找出原因。`,
  "sentence.incident.componentDown.fix.metricsServer": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}沒有就緒的副本，從事件和日誌找出原因。`,
  "sentence.incident.nodeNotReady.cause": (p: P) =>
    `${p.status === "Unknown" ? `節點 ${p.node ?? ""} 不再向叢集回報狀態` : `節點 ${p.node ?? ""} 未就緒`}${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}。`,
  "sentence.incident.nodeNotReady.why": (p: P) =>
    `節點未就緒時不會再啟動新的副本。寬限期（預設 5 分鐘）過後，叢集會移除上面的副本，讓副本到別處重新啟動。這個節點上有 ${p.pods ?? ""} 個副本，${p.services ?? ""} 個服務有副本在上面。`,
  "sentence.incident.nodeNotReady.fix": (p: P) =>
    `檢查 ${p.node ?? ""} 背後的機器：是否開機、網路，以及節點代理程式（kubelet）。如果機器無法恢復，就清空這個節點，再換一台。`,
  "sentence.trace.dns.ok": (p: P) => `${p.host ?? ""} 可以解析。`,
  "sentence.trace.dns.notFound": (p: P) =>
    `從叢集內解析不到 ${p.host ?? ""}。叢集外的 DNS 可能查得到，所以 Zeabur 無法確定。`,
  "sentence.trace.dns.unknown": (p: P) => `沒有查詢 ${p.host ?? ""}，或查詢沒有及時回應。`,
  "sentence.trace.address.noStatus": (p: P) =>
    `${p.route ?? ""} 的狀態裡還沒有位址，無法和網域的位址比對。`,
  "sentence.trace.address.unresolved": (p: P) =>
    `網域沒有解析出位址，無法和 ${p.route ?? ""} 的位址比對。`,
  "sentence.trace.address.ok": (p: P) => `網域指向 ${p.route ?? ""} 提供服務的位址。`,
  "sentence.trace.address.elsewhere": (p: P) =>
    `網域指向的位址不屬於 ${p.route ?? ""}。如果叢集前面有 CDN 或代理伺服器，回應的會是它們的位址。`,
  "sentence.trace.controller.classesUnreadable": (p: P) =>
    `Zeabur 無法讀取叢集的 IngressClass，所以無法判斷哪個控制器負責 ${p.route ?? ""}。`,
  "sentence.trace.controller.classMissing": (p: P) =>
    `${p.route ?? ""} 指定的 IngressClass ${p.class ?? ""} 不在叢集裡。`,
  "sentence.trace.controller.unreadable": (p: P) =>
    `Zeabur 無法讀取叢集的元件，所以無法判斷 ${p.route ?? ""} 的 Ingress 控制器是否在執行。`,
  "sentence.trace.controller.notRecognised": (p: P) =>
    `Zeabur 認不出負責 ${p.route ?? ""} 的 Ingress 控制器${!p.controller || p.controller === "none" ? `` : ` ${p.controller ?? ""}`}，所以無法判斷控制器是否在執行。`,
  "sentence.trace.controller.ok": (p: P) => `Ingress 控制器 ${p.workload ?? ""} 有就緒的副本。`,
  "sentence.trace.controller.down": (p: P) =>
    `Ingress 控制器 ${p.workload ?? ""} 沒有就緒的副本，所以沒有東西處理 ${p.route ?? ""}。`,
  "sentence.trace.certificate.defaultCertificate": (p: P) =>
    `${p.route ?? ""} 為 ${p.host ?? ""} 啟用 HTTPS，卻沒有指定機密變數，所以控制器會改用自己的預設憑證。`,
  "sentence.trace.certificate.ok": (p: P) =>
    `${p.host ?? ""} 的憑證已簽發${!p.until || p.until === "none" ? `` : `，有效期限到 ${p.until ?? ""}`}。`,
  "sentence.trace.certificate.notReady": (p: P) =>
    `${p.host ?? ""} 的憑證 ${p.certificate ?? ""} 尚未簽發${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}。`,
  "sentence.trace.certificate.issuing": (p: P) =>
    `cert-manager 正在為 ${p.host ?? ""} 簽發憑證 ${p.certificate ?? ""}。`,
  "sentence.trace.certificate.pending": (p: P) =>
    `${p.host ?? ""} 的憑證 ${p.certificate ?? ""} 還沒有結果。`,
  "sentence.trace.certificate.unreadable": (p: P) =>
    `Zeabur 無法讀取專案的機密變數，所以無法檢查機密變數 ${p.secret ?? ""} 裡 ${p.host ?? ""} 的憑證。`,
  "sentence.trace.certificate.notChecked": (p: P) =>
    `機密變數 ${p.secret ?? ""} 存有 ${p.host ?? ""} 的憑證，Zeabur 不會進一步檢查。`,
  "sentence.trace.certificate.secretMissing": (p: P) =>
    `${p.route ?? ""} 從機密變數 ${p.secret ?? ""} 讀取 ${p.host ?? ""} 的憑證，但專案裡沒有這個機密變數。控制器可能改用預設憑證，Zeabur 無法檢查用的是哪一張。`,
  "sentence.trace.route.noStatus": (p: P) =>
    `還沒有任何元件寫入 ${p.route ?? ""} 的狀態，無法得知是否已生效。`,
  "sentence.trace.route.ok": (p: P) => `${p.route ?? ""} 已生效：控制器已在路由的狀態裡寫入位址。`,
  "sentence.trace.route.accepted": (p: P) => `Gateway 已接受 ${p.route ?? ""}。`,
  "sentence.trace.route.notAccepted": (p: P) =>
    `${p.route ?? ""} 的流量入口回報 ${p.condition ?? ""} ${p.reason ?? ""}，請求無法通過。`,
  "sentence.trace.service.unreadable": (p: P) =>
    `Zeabur 無法讀取專案的私有位址（Service），所以無法檢查 ${p.endpoint ?? ""}。`,
  "sentence.trace.service.missing": (p: P) =>
    `找不到名為 ${p.endpoint ?? ""} 的私有位址（Service）。`,
  "sentence.trace.service.portMissing": (p: P) =>
    `私有位址 ${p.endpoint ?? ""} 沒有連接埠 ${p.port ?? ""}。`,
  "sentence.trace.service.selectsOther": (p: P) =>
    `私有位址 ${p.endpoint ?? ""} 把流量送到其他副本，而不是 ${p.service ?? ""} 的副本。`,
  "sentence.trace.service.ok": (p: P) =>
    `私有位址 ${p.endpoint ?? ""} 把流量送到 ${p.service ?? ""}。`,
  "sentence.trace.targetPort.unknown": (p: P) =>
    `Zeabur 讀不到容器，所以沒有檢查 ${p.endpoint ?? ""} 轉送到的連接埠。`,
  "sentence.trace.targetPort.notReached": (p: P) =>
    `沒有檢查：流量在到達 ${p.endpoint ?? ""} 的連接埠之前就停了。`,
  "sentence.trace.targetPort.mismatch": (p: P) =>
    `${p.endpoint ?? ""} 把流量送到連接埠 ${p.targetPort ?? ""}，但容器沒有宣告這個連接埠${!p.ports || p.ports === "none" ? `` : `，宣告的是 ${p.ports ?? ""}`}。`,
  "sentence.trace.targetPort.undeclared": (p: P) =>
    `容器沒有宣告任何連接埠，無法得知容器是否在 ${p.endpoint ?? ""} 轉送到的連接埠上監聽。`,
  "sentence.trace.targetPort.ok": (p: P) => `${p.endpoint ?? ""} 把流量送到容器宣告的連接埠。`,
  "sentence.trace.ready.scaledToZero": (p: P) => `${p.service ?? ""} 的副本數設為 0。`,
  "sentence.trace.ready.ok": (p: P) =>
    `${p.service ?? ""} 有 ${p.ready ?? ""}/${p.pods ?? ""} 個副本就緒。`,
  "sentence.trace.ready.starting": (p: P) => `${p.service ?? ""} 正在啟動，還沒有就緒的副本。`,
  "sentence.trace.ready.noneReady": (p: P) => `${p.service ?? ""} 沒有任何就緒的副本。`,
  "sentence.diagnosis.unreachable.cause.ingressClassMissing": (p: P) =>
    `送到 ${p.route ?? ""} 網域的請求到不了 ${p.service ?? ""}：路由指定的 IngressClass ${p.class ?? ""} 不在叢集裡。`,
  "sentence.diagnosis.unreachable.why.ingressClassMissing": () =>
    "Ingress 控制器只處理自己類別的 Ingress，沒有這個類別，就沒有控制器會處理。",
  "sentence.diagnosis.unreachable.fix.ingressClassMissing": (p: P) =>
    `把 ${p.route ?? ""} 改用叢集裡現有的類別，或安裝 ${p.class ?? ""} 的控制器。`,
  "sentence.diagnosis.unreachable.cause.controllerDown": (p: P) =>
    `送到 ${p.route ?? ""} 網域的請求到不了 ${p.service ?? ""}：Ingress 控制器 ${p.workload ?? ""} 沒有就緒的副本。`,
  "sentence.diagnosis.unreachable.why.controllerDown": () =>
    "Ingress 控制器負責把網域的請求帶進叢集。控制器沒有任何就緒的副本時，這些請求就得不到回應。",
  "sentence.diagnosis.unreachable.fix.controllerDown": (p: P) =>
    `${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : `（${p.namespace ?? ""}）`}沒有就緒的副本，從事件和日誌找出原因。`,
  "sentence.diagnosis.unreachable.cause.certificateNotReady": (p: P) =>
    `${p.host ?? ""} 的 HTTPS 連線失敗：憑證 ${p.certificate ?? ""} 尚未簽發${!p.reason || p.reason === "none" ? `` : `（${p.reason ?? ""}）`}。`,
  "sentence.diagnosis.unreachable.why.certificateNotReady": (p: P) =>
    `cert-manager 簽發憑證後，才會寫進機密變數 ${p.secret ?? ""}。在那之前，控制器會改用預設憑證，瀏覽器會拒絕。`,
  "sentence.diagnosis.unreachable.fix.certificateNotReady": (p: P) =>
    `憑證 ${p.certificate ?? ""} 的狀態和事件會說明為什麼還沒簽發。`,
  "sentence.diagnosis.unreachable.cause.backendPortMissing": (p: P) =>
    `通往 ${p.service ?? ""} 的路由指定了連接埠 ${p.port ?? ""}，但私有位址 ${p.endpoint ?? ""} 沒有這個連接埠。`,
  "sentence.diagnosis.unreachable.why.backendPortMissing": () =>
    "路由會用編號或名稱把流量送到私有位址的某個連接埠。私有位址沒有的連接埠收不到任何流量。",
  "sentence.diagnosis.unreachable.fix.backendPortMissing": (p: P) =>
    `讓路由指向 ${p.endpoint ?? ""} 有的連接埠，或在 ${p.endpoint ?? ""} 加上連接埠 ${p.port ?? ""}。`,
  "sentence.diagnosis.unreachable.cause.selectsOther": (p: P) =>
    `通往 ${p.service ?? ""} 的路由把流量送到 ${p.endpoint ?? ""}，但 ${p.endpoint ?? ""} 會送到其他副本，而不是 ${p.service ?? ""} 的副本。`,
  "sentence.diagnosis.unreachable.why.selectsOther": () =>
    "私有位址只會把流量送到標籤符合 selector 的副本。",
  "sentence.diagnosis.unreachable.fix.selectsOther": (p: P) =>
    `修改 ${p.endpoint ?? ""} 的 selector，讓 selector 符合 ${p.service ?? ""} 的標籤，或讓路由改指向 ${p.service ?? ""} 自己的私有位址。`,
  "sentence.diagnosis.unreachable.cause.scaledToZero": (p: P) =>
    `沒有流量到得了 ${p.service ?? ""}：副本數設為 0。`,
  "sentence.diagnosis.unreachable.why.scaledToZero": () =>
    "私有位址只會把流量送到執行中的副本，而目前設定不執行任何副本。",
  "sentence.diagnosis.unreachable.fix.scaledToZero": (p: P) =>
    `把 ${p.service ?? ""} 的副本數調到至少 1。`,
  "sentence.gap.ephemeralData.none": (p: P) =>
    `${p.container ?? ""} 把 ${p.software ?? ""} 資料存在 ${p.path ?? ""}，但這個路徑不在任何磁碟區上，副本一換掉，資料就不見了。`,
  "sentence.gap.ephemeralData.ephemeral": (p: P) =>
    `${p.container ?? ""} 把 ${p.software ?? ""} 資料存在 ${p.path ?? ""}，但這個磁碟區只跟著副本存在，副本一換掉，資料就不見了。`,
}
