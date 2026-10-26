// English: the source of every message key. The other locales are typed as `Messages`, so a key
// missing there (or added there only) is a TypeScript error.
import type { P } from "@/i18n/sentence-parts"
import { core } from "@/i18n/en-core"
import { serverEn } from "@/i18n/server/en"

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`
const listEn = (items: string[]) => new Intl.ListFormat("en").format(items)
const weekdayEn = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]
const sentenceCase = (text: string) => text.charAt(0).toUpperCase() + text.slice(1)

/** What a refused verb does, in a phrase: `patch` changes, `create` adds. */
const verbEn: Record<string, string> = {
  get: "read",
  list: "list",
  watch: "watch",
  create: "add",
  update: "change",
  patch: "change",
  delete: "delete",
}

/** The object a problem names: its name, else its resource (in its namespace). */
const thingEn = (p: P) =>
  p.name ?? (p.resource ? `${p.resource}${p.namespace ? ` in ${p.namespace}` : ""}` : "this")

/** A wait in its largest unit, rounded up: 90 s is 2 minutes. */
/** A container by its service, or the service alone when the container has its name. */
const containerEn = (p: P) =>
  !p.service || p.container === p.service
    ? (p.service ?? p.container)
    : `${p.container} in ${p.service}`

const waitEn = (s: number) =>
  s < 60
    ? plural(s, "second", "seconds")
    : s < 3600
      ? plural(Math.ceil(s / 60), "minute", "minutes")
      : plural(Math.ceil(s / 3600), "hour", "hours")

export const en = {
  // Diagnoses, gaps, traces and incidents: src/i18n/server
  ...serverEn,
  // The shell: src/i18n/en-core.ts
  ...core,
  // Shell
  "shell.findProject": "Find a project",
  "shell.switchProject": "Switch project",
  "column.label": "Services",
  "column.other": "Other objects",
  "column.advanced": "Advanced",
  "column.namespace": "Kubernetes namespace",

  // Common
  "common.colon": ": ",
  "common.retry": "Try again",
  "common.all": "All",
  "common.copy": "Copy",
  "common.copied": "Copied",
  "common.status": "Status",
  "common.sep": ", ",
  /** Between a sentence and what follows it, such as a link. */
  "common.gap": " ",
  /** Whole sentences, one after another. */
  "common.sentences": (sentences: (string | undefined)[]) => sentences.filter(Boolean).join(" "),
  "common.list": (items: string[]) => new Intl.ListFormat("en").format(items),
  "common.more": (n: number) => `${n} more`,
  "common.domains": "Domains",
  "common.newTab": "(opens in a new tab)",
  "common.managedBy": (tool: string) => `Managed by ${tool}`,
  "common.managedByDetail": (tool: string) =>
    `Changes made outside ${tool} are overwritten on its next sync or upgrade.`,

  // Problem
  "problem.whatToDo": "What to do",
  "problem.kubernetes": "Details for Kubernetes users",
  "problem.code": "HTTP status",
  "problem.reason": "Reason",
  "problem.message": "Message",
  "problem.title.bad-request": "The request is not valid",
  "problem.title.unauthenticated": "You are not signed in",
  "problem.title.signed-out": "You are signed out",
  "problem.title.forbidden": "You do not have permission",
  "problem.title.quota-exceeded": "This would go over the project's limit",
  "problem.title.policy-denied": "A policy in the cluster refused this",
  "problem.title.not-found": "This object doesn't exist",
  "problem.title.log-unavailable": "The log is not available",
  "problem.title.conflict": "It changed in the meantime",
  "problem.title.managed": "Another tool manages it",
  "problem.title.plan-expired": "These changes expired",
  "problem.title.system-namespace": "Kubernetes keeps this name for itself",
  "problem.title.invalid": "The cluster rejected the change",
  "problem.title.too-many-requests": "The cluster is busy",
  "problem.title.unavailable": "The cluster cannot be reached",
  "problem.title.internal": "Something went wrong in Zeabur",
  "problem.fix.bad-request": "Check the request and try again.",
  "problem.fix.unauthenticated": "Sign in again.",
  "problem.fix.signed-out": "Sign in again.",
  "problem.fix.forbidden": "Ask a cluster administrator for access, or use an account that has it.",
  "problem.fix.quota-exceeded":
    "Ask for less, free some of what the project uses, or ask a cluster administrator to raise the limit.",
  "problem.fix.policy-denied":
    "Change what the policy names, or ask a cluster administrator about the policy.",
  "problem.fix.not-found": "Reload the page to see what the cluster has now.",
  "problem.fix.log-unavailable": "Read the current run's log, or wait a moment and refresh.",
  "problem.fix.conflict": "Reload to see the latest version, then make the change again.",
  "problem.fix.managed":
    "Change it where the managing tool reads it, such as Helm values or a Git repository. Restarting still works here.",
  "problem.fix.plan-expired": "Review the changes again.",
  "problem.fix.system-namespace": "Choose a name that doesn't start with kube-.",
  "problem.fix.invalid": "Correct the fields the cluster names in the details, then try again.",
  "problem.fix.too-many-requests": "Wait a moment and try again.",
  "problem.fix.unavailable":
    "Check that the cluster is running and that this machine can reach it.",
  "problem.fix.internal": "Try again. If it keeps happening, report it with the details.",
  "problem.detail": "Zeabur's message",
  "problem.about": "What this kind of problem means",
  "problem.retryAfter": (s: number) => `The cluster asked to wait ${waitEn(s)}.`,
  // A problem type's page (/problems/{kind})
  "problemPage.label": "Problem type",
  "problemPage.rule": "The rule",
  "problemPage.kubernetes": "In the Kubernetes documentation",
  "problemPage.rule.bad-request":
    "Zeabur checks each request before anything reaches the cluster: what it asks to change must be complete and well formed. A request that is not is refused, and nothing is written.",
  "problemPage.rule.unauthenticated":
    "Every request reaches the cluster with your own sign-in. When the cluster no longer accepts it (it expired, or was revoked), nothing can be read or changed until you sign in again.",
  "problemPage.rule.signed-out":
    "Your Zeabur session ended, so Zeabur can no longer act for you in the cluster. Nothing is read or changed until you sign in again.",
  "problemPage.rule.forbidden":
    "The cluster decides what each person may do, by rules (RBAC) that say who may read or change which kinds of objects, in which projects. Zeabur acts only with your own rights: what they do not allow, it cannot do for you.",
  "problemPage.rule.quota-exceeded":
    "A project can have limits on what all its services may reserve together: CPU, memory, the number of replicas or objects. The cluster refuses a change that would go over them, whatever your rights.",
  "problemPage.rule.policy-denied":
    "A cluster can check every change against policies of its own, run by an admission webhook or a policy engine. When one refuses a change, its own words say why. Your rights do not change its answer.",
  "problemPage.rule.not-found":
    "What was asked for is not in the cluster: it may have been deleted or renamed since the page read it.",
  "problemPage.rule.log-unavailable":
    "The cluster keeps a container's log for its current run and the run before, and only while its replica exists. A log never written, or one whose replica is gone, cannot be read.",
  "problemPage.rule.conflict":
    "Zeabur writes a change only onto the version you reviewed. When someone or something (a deploy tool, an autoscaler) changed the object after that, nothing is written, so no one's change is lost.",
  "problemPage.rule.managed":
    "Another tool (Helm, Argo CD, Flux) owns this object and puts its own version back at its next sync or upgrade. A change made here would be undone, so Zeabur refuses it.",
  "problemPage.rule.plan-expired":
    "Zeabur writes the changes exactly as you reviewed them. They expire after a few minutes, so what is applied still matches the cluster.",
  "problemPage.rule.system-namespace":
    "Kubernetes keeps names starting with kube- (kube-system, kube-public, kube-node-lease) for its own components. They cannot become projects.",
  "problemPage.rule.invalid":
    "The cluster checks each object against its schema and rules before it stores it. It refused this one, and named the fields that are not valid.",
  "problemPage.rule.too-many-requests":
    "The cluster limits how many requests it serves at once, so it stays responsive. It turned this one away for now; nothing was changed.",
  "problemPage.rule.unavailable":
    "Zeabur could not reach the cluster's API server: it may be down or restarting, or out of reach from where Zeabur runs.",
  "problemPage.rule.internal":
    "Something failed inside Zeabur itself, not in the cluster. The details hold what Zeabur knows about it.",
  "refused.rights": "This needs more rights.",
  "refused.rightsFor": (what: string) => `${what} needs more rights.`,
  "refused.ask": "Ask a cluster administrator.",
  "refused.details": "Details",
  "refused.missing": "Missing rights",
  "refused.see": (what: string) => `Seeing ${what}`,
  "refused.some": "Some of this page",
  "refused.terminal": "Opening a terminal",
  "refused.reveal": "Showing secret values",
  "refused.listThese": "Listing these",
  // Status words
  "status.ready": "Ready",
  "status.rollingOut": "Rolling out",
  "status.failing": "Failing",
  "status.degraded": "Degraded",
  "status.scheduled": "Scheduled",
  "status.paused": "Paused",
  "status.rolloutStuck": "Rollout stuck",
  "status.failed": "Failed",
  "status.building": "Building",
  "status.waitingUpload": "Waiting for upload",
  "status.scaledToZero": "Scaled to 0",
  "status.completed": "Completed",
  "status.unknown": "Unknown",
  "status.lastExit": (code: number, ago: string) =>
    ago ? `last exit ${code}, ${ago}` : `last exit ${code}`,
  // A failing pod's reason
  "reason.crashing": "Keeps crashing",
  "reason.runFailed": "Run failed",
  "reason.outOfMemory": "Out of memory",
  "reason.imagePull": "Can't download image",
  "reason.buildFailed": "Build failed",
  "reason.imageName": "Image name is wrong",
  "reason.configMissing": "Missing config",
  "reason.cantStart": "Can't start",
  "reason.waitingForRoom": "Waiting for room",
  "reason.starting": "Starting",
  "reason.evicted": "Moved off its machine",
  "reason.releaseStuck": "Rollout stuck",
  "reason.gaveUp": "Stopped retrying",
  "reason.security": "Blocked by security settings",
  "phase.Running": "Running",
  "phase.Progressing": "Progressing",
  "phase.Degraded": "Degraded",
  "phase.Paused": "Paused",
  "phase.Suspended": "Suspended",
  "phase.Completed": "Completed",
  "phase.Failed": "Failed",
  "phase.Unknown": "Unknown",

  // Roles
  "role.web": "Web",
  "role.internal": "Private services",
  "role.worker": "Background",
  "role.job": "Jobs",
  "role.data": "Data",

  // Metrics
  "metric.restarts": "Restarts",
  "metric.warnings": "Warnings",
  "metric.services": "Services",

  "home.title": "Home",
  "home.status": "Status",
  "home.rollingOut": (n: number) => `${n} rolling out`,
  "home.nodesReady": (ready: number, total: number) => `${ready}/${total} nodes ready`,
  "home.allProjects": "All projects",
  "home.warningsHour": "Warnings, 1 h",
  "home.noEventsPermission": "You cannot list the cluster's events.",
  "home.noServices": "No services yet",

  // Cluster
  "cluster.title": "Cluster",
  "cluster.tabs": "Cluster pages",
  "cluster.overview": "Overview",
  "cluster.catalogue": "Catalog",
  "cluster.endOfLife": "End of life",
  "cluster.endOfLifeOn": (date: string) => `End of life ${date}`,
  "cluster.skewBadge": "Unsupported node version",
  "cluster.upgrading": "Upgrading",
  "cluster.available": (version: string) => `${version} available`,
  "cluster.patch": (version: string) => `Patch ${version}`,
  "cluster.nextMinor": (version: string) => `Next minor ${version}`,
  "cluster.allClear": "Nothing needs attention.",
  "cluster.more": (n: number) => `${n} more`,
  "cluster.fewer": "Fewer",
  "cluster.attEolSoon": (minor: string, date: string) =>
    `Kubernetes ${minor} reaches end of life on ${date}`,
  "cluster.attEolPast": (minor: string, date: string) =>
    `Kubernetes ${minor} reached end of life on ${date}`,
  "cluster.attEolOld": (minor: string) => `Kubernetes ${minor} is past end of life`,
  "cluster.attSkew": (node: string, version: string) =>
    `${node} runs kubelet ${version}, outside the supported version skew`,
  "cluster.attComponent": (name: string, version: string, tested: string) =>
    `${name} ${version} is older than the tested ${tested}`,
  "cluster.attEol.why": "It no longer gets security fixes.",
  "cluster.attEolSoon.why": "After that it gets no security fixes.",
  "cluster.attEol.fix": "Upgrade to a supported minor version.",
  "cluster.attSkewMany": (n: number) =>
    `${n} nodes run a kubelet outside the supported version skew`,
  "cluster.attSkew.why": "Kubernetes does not support it, so replicas there may misbehave.",
  "cluster.attSkew.fix": "Upgrade the kubelet to the control plane's minor version.",
  "cluster.attComponents": (n: number) => `${n} components are older than the tested versions`,
  "cluster.attComponent.why": "It still runs, but Zeabur was tested with a newer version.",
  "cluster.attComponent.fix": "Upgrade it under Components.",
  "cluster.versionOf": (version: string, tested: string) => `${version}, tested ${tested}`,
  "cluster.showMembers": (n: number) => `Show all ${n}`,
  "cluster.notChecked":
    "Nothing found in what could be read, but part of the cluster could not be checked.",
  "cluster.cannotCheck": "Cannot check: the cluster cannot be reached.",
  "cluster.notRead": "Not read: the cluster cannot be reached.",
  "cluster.apiServer": "API server",
  "cluster.whatApiServer": "the API server's readiness",
  "cluster.apiReady": "Ready",
  "cluster.apiFailing": (n: number) => plural(n, "check failing", "checks failing"),
  "cluster.apiCheckFailed": "failed",
  "cluster.apiUnknown": "Unknown: the API server's readiness could not be read.",
  "cluster.apiCovers": (n: number) =>
    `${plural(n, "readiness check", "readiness checks")} on the API server that answered. If the cluster has more than one API server, the others are not shown here.`,
  "cluster.apiLeases":
    "The scheduler and controller manager are checked separately: each is running if its lease was renewed recently.",
  "cluster.apiNotCovered": "The scheduler and controller manager are not checked here.",
  "cluster.leaseRenewed": "Running, lease renewed",
  "cluster.leaseStale": "May have stopped, lease last renewed",
  "node.notReady": (n: number, node: string) =>
    n === 1 ? `${node} is not ready` : `${n} nodes are not ready`,
  "node.notReady.why":
    "Services with replicas there are down until it is back or the replicas move.",
  "node.notReady.fix":
    "Check that the machine is running and its kubelet can reach the control plane.",
  "node.memoryPressure": (n: number, node: string) =>
    n === 1 ? `${node} is out of memory` : `${n} nodes are out of memory`,
  "node.memoryPressure.why": "The kubelet is stopping replicas to free memory.",
  "node.memoryPressure.fix":
    "Stop or limit the services that use the most memory, or add memory to the server.",
  "node.diskPressure": (n: number, node: string) =>
    n === 1 ? `${node} is almost out of disk space` : `${n} nodes are almost out of disk space`,
  "node.diskPressure.why":
    "The kubelet is removing unused images and stopping replicas to free space; new replicas cannot start.",
  "node.diskPressure.fix":
    "Free space: remove unused images (crictl rmi --prune), old logs or volumes, or grow the disk.",
  "node.pidPressure": (n: number, node: string) =>
    n === 1 ? `${node} is running out of processes` : `${n} nodes are running out of processes`,
  "node.pidPressure.why": "New processes and replicas may fail to start.",
  "node.pidPressure.fix":
    "Find the service that starts too many processes, and restart or limit it.",
  "node.networkUnavailable": (n: number, node: string) =>
    n === 1 ? `${node} has no replica network` : `${n} nodes have no replica network`,
  "node.networkUnavailable.why": "Replicas there cannot reach other replicas or services.",
  "node.networkUnavailable.fix": "Check the network plugin on the node.",
  "node.memoryFull": (n: number, node: string, share: string) =>
    n === 1
      ? `${node}: memory ${share} used`
      : `${n} nodes have memory nearly full (up to ${share})`,
  "node.memoryFull.warn": "When it fills up, services are stopped to free memory.",
  "node.memoryFull.fail": "Services may be stopped for lack of memory at any moment.",
  "node.memoryFull.fix":
    "Set memory limits on the services that use the most, stop one you do not need, or add memory.",
  "node.diskFull": (n: number, node: string, share: string) =>
    n === 1 ? `${node}: disk ${share} full` : `${n} nodes have disks nearly full (up to ${share})`,
  "node.diskFull.warn": "From 90% the kubelet stops replicas to free space.",
  "node.diskFull.fail": "The kubelet is about to stop replicas to free space.",
  "node.diskFull.fix":
    "Remove unused images (crictl rmi --prune), old logs or volumes, or grow the disk.",
  "node.cordoned": (n: number, node: string) =>
    n === 1 ? `${node} is cordoned` : `${n} nodes are cordoned`,
  "node.cordoned.why": "New replicas are not placed there; the running ones stay.",
  "node.cordoned.fix": (n: number, node: string) =>
    n === 1
      ? `To move what is left, run kubectl drain ${node} in the cluster's kubectl terminal. When the work is done, resume scheduling from its menu.`
      : "To move what is left, run kubectl drain on each node in the cluster's kubectl terminal. When the work is done, resume scheduling from each node's menu.",
  "node.reservedMemory": (n: number, node: string, share: string) =>
    n === 1 ? `${node}: memory ${share} reserved` : `${n} nodes have almost all memory reserved`,
  "node.reservedMemory.why": "New replicas that ask for memory may wait for room.",
  "node.reservedMemory.fix": "Lower the memory requests of the services there, or add a node.",
  "node.reservedCpu": (n: number, node: string, share: string) =>
    n === 1 ? `${node}: CPU ${share} reserved` : `${n} nodes have almost all CPU reserved`,
  "node.reservedCpu.why": "New replicas that ask for CPU may wait for room.",
  "node.reservedCpu.fix": "Lower the CPU requests of the services there, or add a node.",
  "unreachable.title": (name: string) => `Cannot reach ${name}`,
  "unreachable.zeabur": "Zeabur does not respond",
  "unreachable.lastHeard": "Last answered",
  "unreachable.retrying": "Trying again…",
  "unreachable.every": "Zeabur tries again every 15 seconds.",
  "cluster.noProjects": "No projects yet",
  "cluster.noProjectsBody":
    "A project is a Kubernetes namespace shown in Zeabur. Create one on the Projects page, or use an existing namespace below.",
  "cluster.openProjects": "Open projects",
  "cluster.nodes": "Nodes",
  "cluster.cpu": "CPU",
  "cluster.memory": "Memory",
  "cluster.disk": "Disk",
  "cluster.usedOf": (used: string, whole: string) => `${used} of ${whole} used`,
  "cluster.reservedShare": (share: string) => `${share} reserved`,
  "cluster.diskState.not-configured": "Disk use is not shown: there is no metrics store.",
  "cluster.diskState.unreadable": "Disk use is not shown: reading the metrics store was refused.",
  "cluster.diskState.unavailable": "Disk use is not shown: the metrics store did not respond.",
  "cluster.diskState.not-collected":
    "Disk use is not shown: the metrics store has no node-exporter filesystem metrics for these nodes.",
  "cluster.sortNodes": "Sort nodes",
  "cluster.sortWorst": "Worst first",
  "cluster.sortName": "Name",
  "cluster.filterNodes": "Filter nodes",
  "cluster.noNodeMatch": (query: string) => `No nodes match “${query}”.`,
  "node.word.notReady": "Not ready",
  "node.word.memoryPressure": "Out of memory",
  "node.word.diskPressure": "Out of disk",
  "node.word.pidPressure": "Out of processes",
  "node.word.networkUnavailable": "No network",
  "node.word.memoryFull": "Memory nearly full",
  "node.word.diskFull": "Disk nearly full",
  "node.word.cordoned": "Cordoned",
  "node.word.reservedMemory": "Memory over-reserved",
  "node.word.reservedCpu": "CPU over-reserved",
  "node.word.maintenance": "In maintenance",
  "node.word.draining": "Moving replicas off",
  "node.drainingLeft": (n: number) => `Moving replicas off, ${n} left`,
  "node.pinnedLeft": (n: number) =>
    n === 1
      ? "1 replica stays: its data is only on this node"
      : `${n} replicas stay: their data is only on this node`,
  "node.maintenance": (n: number, node: string) =>
    n === 1 ? `${node} is in maintenance` : `${n} nodes are in maintenance`,
  "node.maintenance.why": "No replica is left to move there, and no new one is placed there.",
  "node.maintenance.fix": "When the maintenance is done, resume scheduling from the node's menu.",
  "node.draining": (n: number, node: string) =>
    n === 1 ? `Replicas are moving off ${node}` : `Replicas are moving off ${n} nodes`,
  "node.draining.why": "Their services start them again on other nodes.",
  "node.draining.fix": "They move while kubectl drain runs. If it stopped, run it again.",
  "cluster.cores": (n: string) => `${n} cores`,
  "cluster.requestedOf": (requested: string, allocatable: string) =>
    `${requested} of ${allocatable} reserved`,
  "cluster.allocatable": (allocatable: string) => `${allocatable} allocatable`,
  "cluster.used": (used: string) => `${used} used`,
  "cluster.limits": (limits: string) => `${limits} maximum`,
  "cluster.podsOf": (placed: string, room: string) => `${placed} of ${room} replicas`,
  "cluster.pods": "Replicas",
  "cluster.worker": "worker",
  "cluster.kubelet": "kubelet",
  "cluster.skewMixed": "Supported, though its minor version differs from the control plane's.",
  "cluster.skewUnsupported":
    "Unsupported: newer than the control plane, or more than 3 minor versions older.",
  "cluster.noMetrics": "Install metrics-server to see usage.",
  "cluster.metricsUnavailable": "metrics-server did not respond.",
  "cluster.noUsageYet": "No usage measured yet.",
  "cluster.partialPods": (n: number) => `Reserved amounts count the first ${n} replicas only.`,
  "cluster.unscheduled": (n: number) => `${n} replicas waiting for a node`,
  "cluster.warnings": "Warnings in the last hour",
  "home.showWarnings": (n: number) => `Show ${plural(n, "warning", "warnings")}`,
  "cluster.noWarnings": "No warnings in the last hour.",
  "cluster.moreWarnings": (n: number) => `${n} more`,
  "cluster.partialEvents": "Only the newest 5,000 events were read.",
  "cluster.noComponents":
    "No component from the catalog is installed yet. Pick one below to see how to install it.",
  "cluster.tested": (version: string) => `Tested ${version}`,
  "cluster.testedVersion": "Tested version",
  "cluster.newerThanTested": "Newer than tested",
  "cluster.sourceK3s": "K3s",
  "cluster.sourceRke2": "RKE2",
  "cluster.sourceHelmChart": "HelmChart",
  "cluster.sourceHelm": "Helm",
  "cluster.sourceOther": "—",
  "cluster.roleReadsSecrets":
    "This role can read every secret in the cluster: passwords, keys, and tokens. Grant it only to someone you would trust as a cluster administrator.",
  "cluster.copyRole": "Copy ClusterRole",
  "cluster.whatProjects": "the projects",
  "cluster.whatNodes": "the nodes",
  "cluster.whatRequests": "what replicas reserve",
  "cluster.whatUpgrades": "the upgrade plans",
  "cluster.whatUsage": "node usage",
  "cluster.whatWarnings": "the cluster's warnings",
  "cluster.whatComponents": "the installed components",
  "clusterUsage.title": "Resource usage",
  "clusterUsage.sort": "Sort by",
  "clusterUsage.mostMemory": (name: string, amount: string, share?: string) =>
    share
      ? `${name} uses the most memory: ${amount}, ${share} of the cluster.`
      : `${name} uses the most memory: ${amount}.`,
  "clusterUsage.mostCpu": (name: string, amount: string, share?: string) =>
    share
      ? `${name} uses the most CPU: ${amount}, ${share} of the cluster.`
      : `${name} uses the most CPU: ${amount}.`,
  "clusterUsage.reservesMostMemory": (name: string, amount: string, share?: string) =>
    share
      ? `${name} reserves the most memory: ${amount}, ${share} of the cluster.`
      : `${name} reserves the most memory: ${amount}.`,
  "clusterUsage.reservesMostCpu": (name: string, amount: string, share?: string) =>
    share
      ? `${name} reserves the most CPU: ${amount}, ${share} of the cluster.`
      : `${name} reserves the most CPU: ${amount}.`,
  "clusterUsage.nothingRunning": "No replicas are running.",
  "clusterUsage.projectsTotal": (amount: string) => `Projects ${amount}`,
  "clusterUsage.othersTotal": (amount: string) => `Other namespaces ${amount}`,
  "clusterUsage.systemTotal": (amount: string) => `Kubernetes system ${amount}`,
  "clusterUsage.projects": "Projects",
  "clusterUsage.others": "Other namespaces",
  "clusterUsage.namespaces": "Namespaces",
  "clusterUsage.system": "Kubernetes system",
  "clusterUsage.pods": (n: number) => plural(n, "replica", "replicas"),
  "clusterUsage.notMeasured": (n: number) => `${n} not measured yet`,
  "clusterUsage.reserved": (amount: string) => `${amount} reserved`,
  "clusterUsage.ofCluster": (share: string) => `${share} of the cluster`,
  "clusterUsage.nearMaximum": "Memory near its maximum",
  "clusterUsage.noServices": "No service in this project has usage to show.",
  "clusterUsage.servicesUnreadable": "Zeabur could not read this project's services.",
  "clusterUsage.unknown.not-installed":
    "Use is unknown: metrics-server is not installed. The rows show what replicas reserve.",
  "clusterUsage.unknown.unavailable":
    "Use is unknown: metrics-server did not respond. The rows show what replicas reserve.",
  "clusterUsage.unknown.unreadable": "The rows show what replicas reserve.",
  "clusterUsage.whatUsage": "what replicas use",
  "clusterUsage.whatPods": "what each namespace uses",
  "clusterUsage.whatProjects": "which namespaces are projects",
  "maint.actions": (node: string) => `Actions for ${node}`,
  "maint.cordon": "Stop scheduling",
  "maint.uncordon": "Resume scheduling",
  "maint.title.cordon": (node: string) => `Stop scheduling on ${node}`,
  "maint.title.uncordon": (node: string) => `Resume scheduling on ${node}`,
  "maint.lead.cordon": (node: string) =>
    `New replicas will not be placed on ${node}. The ones running there keep running.`,
  "maint.lead.uncordon": (node: string) => `New replicas can be placed on ${node} again.`,
  "maint.kube.command": "Command",

  // Projects
  "projects.title": "Projects",
  "projects.adopted": "Project",
  "projects.namespaceOnly": "Namespace",
  "projects.other": "Other namespaces",
  "projects.noOther": "Every namespace is a project.",
  "projects.filterOther": "Filter namespaces",
  "projects.noOtherMatch": (query: string) => `No namespaces match “${query}”.`,
  "projects.noneBody":
    "A project is a Kubernetes namespace shown in Zeabur. Create one, or use an existing namespace below.",
  "projects.new": "New project",
  "projects.newBody":
    "A project is a namespace with the Zeabur project label. You review it before it is created.",
  "projects.newName": "Name",
  "projects.newNameHint": "It becomes the namespace's name, which can't change later.",
  "projects.newDisplayName": "Display name",
  "projects.newDisplayNameHint": "Optional. Shown in place of the name.",
  "projects.problem.empty": "Give it a name.",
  "projects.problem.invalid":
    "Lowercase letters, digits and hyphens, starting and ending with a letter or digit.",
  "projects.problem.long": "At most 63 characters.",
  "projects.problem.system": "Names starting with kube- belong to Kubernetes.",
  "projects.problem.taken": "A namespace of this name exists. Use the existing namespace instead.",
  "projects.planNew": (name: string) => `Create project ${name}`,
  "projects.rowMenu": (name: string) => `More for ${name}`,
  "projects.showSystem": "Show system namespaces",
  "projects.forbiddenTitle": "You cannot list projects",
  "projects.view": "View",
  "projects.viewGrid": "Cards",
  "projects.viewList": "List",
  "projects.columnName": "Name",
  "projects.filter": "Filter projects",
  "projects.noMatch": (query: string) => `No projects match “${query}”.`,

  // Project
  "project.noRole": (name: string) => `You don't have access to ${name}`,
  "project.noRoleBody": "An admin can give you a role in it on the Members page.",
  "project.sections": "Project sections",
  "project.entry.overview": "Overview",
  "project.services": "Services",
  "project.events": "Events",
  "project.noServices": "No services yet",
  "project.seeObjects": "View all objects",
  "project.healthyOf": (healthy: number, total: number) => `${healthy}/${total} healthy`,
  "project.deleting": "Deleting…",
  "deletion.goes": "Goes with the project",
  "deletion.services": (names: string) => `Services: ${names}`,
  "deletion.database": (kind: string) => `Database (${kind})`,
  "deletion.dataDeleted": "Its data is deleted with it",
  "deletion.dataKept": (volume: string) => `Its data stays in PersistentVolume ${volume}`,
  "deletion.dataKeptUnbound": "Its data stays: the StorageClass keeps volumes",
  "deletion.dataUnknown": "Unable to tell whether its data stays",
  "deletion.backUpFirst": "Back up first",
  "deletion.backUpVolume": (claim: string) => `Back up ${claim} now`,
  "deletion.toolsPods": "The kubectl terminal you opened in this project is deleted too",
  "deletion.operatorDown": (names: string) =>
    `No operator is running for ${names}, so their cleanup can't run and the project may take a long time to go away`,
  "deletion.stays": "Stays",
  "deletion.staysBackups": "Backups in S3",
  "deletion.resticPassword":
    "Volume backups in S3 stay, but the restic password that restores them is in a secret in this project and goes with it. Keep a copy first.",
  "deletion.staysOutside":
    "Everything outside the project, such as other projects and cluster settings",
  "deletion.restore": (action: string) =>
    `To get the project back later, use ${action} on the cluster's backups page.`,
  "deletion.openBackups": "Open backups",
  "deleting.title": "Deleting this project",
  "deleting.gone": "This project is deleted",
  "deleting.toProjects": "Back to projects",
  "deleting.body":
    "Kubernetes deletes what's in the namespace, then the namespace itself. This usually takes a minute or two.",
  "deleting.since": (ago: string) => `Started ${ago}.`,
  "deleting.count": (resource: string, count: number) => `${resource} (${count})`,
  "deleting.left": (list: string) => `Left: ${list}.`,
  "deleting.held":
    "These objects wait for their finalizers. The controller that added each finalizer removes it after its cleanup; Zeabur doesn't remove finalizers.",
  "deleting.commandsLabel": "Look with kubectl",

  // Overview
  "attention.title": "Needs attention",
  "attention.ready": (ready: number, desired: number) => `${ready}/${desired} ready`,
  "attention.logsOf": (pod: string) => `Logs of ${pod}`,
  "attention.eventsOf": (pod: string) => `Events of ${pod}`,
  "overview.healthy": "Healthy",
  "overview.failingCount": (n: number) => `${n} failing`,
  "overview.degradedCount": (n: number) => `${n} not ready`,
  "events.show": "Show events",
  "events.warnings": "Warnings",
  "events.noWarnings": "No warnings.",
  "events.empty": "No recent events. Kubernetes keeps events for about an hour.",
  "events.label": "Events, newest first",
  "events.warning": "Warning",
  "events.normal": "Normal",
  "events.forPod": "Replica",
  "events.clearPod": "Show every replica",
  "domains.https": "HTTPS configured",
  "domains.gatewayHttps": "HTTPS decided by the Gateway",
  "resource.unused": "Unused",
  "resource.unusedCount": (n: number) => `${n} unused`,
  "storage.requestedTotal": (size: string) => `${size} requested`,
  "storage.notMounted": "Not mounted by any service",

  // Services table
  "table.service": "Service",
  "table.status": "Status",
  "table.restarts": "Restarts",
  "table.domain": "Domain",
  "table.actions": "Actions",
  "table.kind": "Kind",
  "table.pods": "Replicas",
  "services.search": "Filter services",
  "services.attention": "Needs attention",
  "services.healthy": "Healthy",
  "services.allKinds": "All kinds",
  "services.count": (shown: number, total: number) =>
    shown === total ? plural(total, "service", "services") : `${shown} of ${total}`,
  "services.noMatch": "No services match.",
  "services.showAll": (n: number) => `Show all ${n}`,
  "services.showFewer": "Show fewer",
  "attention.services": (n: number) => plural(n, "service", "services"),
  "attention.eachOne": "Each one",
  "attention.more": (n: number) => `+${n}`,
  "attention.moreOf": (n: number) => `Show ${plural(n, "more service", "more services")}`,
  "events.warningsCount": (n: number) => plural(n, "warning", "warnings"),
  "domains.withoutHttps": (n: number) => `${n} without HTTPS`,
  "domains.filter": "Filter domains",
  "domains.noMatch": "No domains match.",
  "storage.onVolumes": (size: string, n: number) => `${size} on ${plural(n, "volume", "volumes")}`,
  "storage.other": "Other volumes",
  "storage.filter": "Filter volumes",
  "storage.noMatch": "No volumes match.",
  "storage.size": "Size",
  "storage.usedBy": "Used by",
  "backups.unbacked": (missing: number, total: number) =>
    missing === total
      ? total === 1
        ? "The volume isn't backed up."
        : `None of the ${total} volumes is backed up.`
      : `${missing} of ${total} volumes aren't backed up.`,
  "backups.volumes": (n: number) => plural(n, "volume", "volumes"),

  // Schedules
  "cron.minute": "Every minute",
  "cron.minutes": (n: number) => `Every ${n} minutes`,
  "cron.hour": (minute: number) => (minute === 0 ? "Every hour" : `Every hour at minute ${minute}`),
  "cron.hours": (n: number, minute: number) =>
    minute === 0 ? `Every ${n} hours` : `Every ${n} hours at minute ${minute}`,
  "cron.day": (times: string[]) => `Every day at ${listEn(times)}`,
  "cron.week": (days: number[], times: string[]) =>
    `Every ${listEn(days.map((d) => weekdayEn[d] ?? ""))} at ${listEn(times)}`,
  "cron.weekRange": (from: number, to: number, times: string[]) =>
    `${weekdayEn[from]} to ${weekdayEn[to]} at ${listEn(times)}`,
  "cron.month": (days: number[], times: string[]) =>
    `On day ${listEn(days.map(String))} of every month at ${listEn(times)}`,
  "cron.zone": (words: string, zone: string) => `${words} (${zone})`,

  // Architecture
  "lane.web": "Web",
  "lane.internal": "Private services",
  "lane.workers": "Background services",
  "lane.data": "Data",
  "lane.scheduled": "Scheduled",
  "edge.declared": "Configured connection",
  "edge.inferred": "Guessed from variables",
  "edge.route": "its public domain",
  "edge.sharedSecret": (name: string) => `both read the secret ${name}`,
  "edge.sharedConfig": (name: string) => `both read the config file ${name}`,
  "edge.shared": (kind: string, name: string) => `both read ${kind} ${name}`,
  "edge.env": (variable: string) => `named in the variable ${variable}`,
  "edge.reasons": (phrases: string[]) => sentenceCase(phrases.join(", ")),
  "canvas.zoomIn": "Zoom in",
  "canvas.zoomOut": "Zoom out",
  "canvas.fit": "Fit to screen",
  "canvas.legend": "About the lines",
  "canvas.legendHint":
    "Drawn from Kubernetes objects and variable values, not from traffic. Configured connection: an Ingress, or a secret or config file both services read. Guessed from variables: a variable's value names the other service's private address (Kubernetes Service).",
  "architecture.edges": (n: number) => plural(n, "connection", "connections"),
  "quickLook.label": (name: string) => `Quick look: ${name}`,
  "quickLook.open": "Open service page",
  "quickLook.upstream": "Upstream",
  "quickLook.downstream": "Downstream",
  "quickLook.noUpstream": "Nothing connects to this service.",
  "quickLook.noDownstream": "This service connects to no other service.",

  // Service page
  "service.tabs": "Service sections",
  "service.copyName": "Copy name for kubectl",
  "service.more": "More",
  "service.moreTabs": "Show more tabs",
  "service.views": "Views",
  "service.delete": "Delete service",
  "service.notFound": "This service does not exist.",
  "service.created": (date: string) => `Created ${date}`,
  "tab.overview": "Overview",
  "tab.logs": "Logs",
  "tab.deployments": "Deployments",
  "tab.builds": "Builds",
  "tab.networking": "Networking",
  "tab.variables": "Variables",
  "tab.settings": "Settings",
  "tab.terminal": "Terminal",
  "tab.storage": "Storage",
  "tab.events": "Events",
  "tab.usage": "Usage",
  "tab.files": "Files",
  "ops.restart": "Restart",
  "ops.later": "Arrives in a later version.",
  "overview.started": (ago: string) => `Started ${ago}`,
  "builds.status": (build: string) => `Build ${build}`,
  "builds.word.pending": "Waiting to build",
  "builds.word.planning": "Planning",
  "builds.word.building": "Building",
  "builds.word.succeeded": "Built",
  "builds.word.failed": "Build failed",
  "builds.word.cancelled": "Build cancelled",
  "builds.word.none": "Not built yet",
  "builds.word.sourceFailed": "Can't get the source",
  "builds.word.builderMissing": "Builder not found",
  "builds.word.retrying": "Retrying",
  "builds.refused.deployKey":
    "The Git host refused the deploy key. Add it to the repository's deploy keys, without write access.",
  "builds.refused.token":
    "The Git host refused the access token. Check that it can still read the repository.",
  "builds.showKey": "Show deploy key",
  "builds.defaultBranch": "default branch",
  "builds.since": (ago: string, commit: string) =>
    commit ? `Latest build started ${ago}, from ${commit}` : `Latest build started ${ago}`,
  "builds.current": (service: string) => `${service} runs the newest image.`,
  "builds.behind": (service: string, tag: string) =>
    `A newer image is ready: ${tag}. ${service} runs an older one.`,
  "builds.notBuilt": "No Zeabur Builder build makes this service's image.",
  "builds.noImage": "No image has been built yet.",
  "builds.suspended": "This Build is paused.",
  "builds.image": "Newest",
  "builds.container": "Container",
  "builds.builder": "Builder",
  "builds.deploy": "Deploy newest",
  "builds.rebuild": "Build again",
  "builds.word.waitingUpload": "Waiting for upload",
  "builds.waitingUpload": (service: string) =>
    `${service} starts once you upload a folder or an archive.`,
  "builds.sourceUpload": "Uploaded folder",
  "builds.upload": "Upload a folder",
  "builds.uploadBody": (build: string) =>
    `Builds the folder with the settings of ${build}. The service updates when the build finishes.`,
  "builds.managedDeploy": (tool: string) =>
    `${tool} sets this service's image and would set it back. Change it at ${tool}'s source.`,
  "builds.autoUpdate": "Auto-deploy new builds",
  "builds.autoUpdateHint": "Each new build goes live as soon as it finishes.",
  "builds.autoUpdatePolicy": (policy: string) =>
    `Keel updates this service with its ${policy} policy, set outside Zeabur.`,
  "builds.keelMissing": "Auto-deploy needs the Keel component, which this cluster doesn't have.",
  "builds.installKeel": "How to install",
  "builds.history": "History",
  "builds.trigger.webhook": "Pushed",
  "builds.trigger.poll": "New commit",
  "builds.trigger.manual": "Started by hand",
  "builds.took": (seconds: number) =>
    seconds < 60 ? `took ${seconds} s` : `took ${Math.floor(seconds / 60)} min ${seconds % 60} s`,
  "builds.logs": "Logs",
  "builds.hideLogs": "Hide logs",
  "builds.step": "Step",
  "builds.logsGone":
    "Logs are deleted with the build's replica, ten minutes after the build finishes.",
  "builds.noLogsYet": "This step has not started yet.",
  "builds.emptyLog": "Nothing logged.",
  "overview.source": "Source",
  "overview.managedBy": "Managed by",
  "overview.replicas": (ready: number, desired: number) => `Replicas ${ready}/${desired}`,
  "overview.replicaSettings": "Replica settings",
  "overview.noneReady": "No replica is ready",
  "overview.someReady": (ready: number, desired: number) => `${ready} of ${desired} replicas ready`,
  "overview.crashing": (n: number) => plural(n, "replica keeps crashing", "replicas keep crashing"),
  "overview.manage": "Manage",
  "overview.warnings": "Warnings",
  "verdict.problem": (what: string) => `Problem: ${what}`,
  "verdict.trouble.healthCheck": "Health check failing",
  "verdict.trouble.quota": "Over the project's limit",
  "verdict.trouble.unreachable": (domain: string) => `${domain} can't be reached`,
  "verdict.reach.reachable": (list: string) => `${list} can be reached`,
  "verdict.reach.browserReachable": (list: string) => `Your browser can reach ${list}`,
  "verdict.reach.unreachable": (list: string) => `${list} can't be reached`,
  "verdict.reach.browserUnreachable": (list: string) => `Your browser can't reach ${list}`,
  "verdict.reach.unknown": (list: string) => `Not confirmed yet whether ${list} can be reached`,
  "verdict.reach.issuing": (list: string, n: number) =>
    n === 1 ? `Waiting for ${list}'s certificate` : `Waiting for certificates for ${list}`,
  "verdict.reach.domain": (list: string, state: string) => `${list}: ${state}`,
  "verdict.reach.seeRecord": "See the record to add",
  "verdict.reach.seeDetails": "See details",
  "verdict.more": (n: number) => `${n} more`,
  "domain.valid": "Valid configuration",
  "domain.invalid": "Invalid configuration",
  "domain.pending": "Issuing certificate",
  "domain.elsewhere": "Points elsewhere",
  "domain.unknown": "Not confirmed",
  "domain.dns.missing": (host: string) => `No DNS record found for ${host}.`,
  "domain.dns.elsewhere": (list: string) =>
    `DNS points to ${list}, not to the cluster. That's expected behind a CDN or proxy. Otherwise, change the record.`,
  "domain.dns.unknown": "Zeabur couldn't look this domain up from the cluster.",
  "domain.addRecord": "Add this record at your DNS provider:",
  "domain.theRecord": "The record that points it here:",
  "domain.askAdmin":
    "Zeabur doesn't know the cluster's entry address. Ask your administrator which address to point it at.",
  "domain.wildcard": (wildcard: string) =>
    `${wildcard} already covers this name, so it needs no record of its own.`,
  "domain.wildcardMissing": (wildcard: string) =>
    `Ask your administrator to add the ${wildcard} record:`,
  "domain.afterAdding": (host: string) => `After you add it, point ${host} here with this record:`,
  "domain.record.type": "Type",
  "domain.record.name": "Name",
  "domain.record.value": "Value",
  "domain.cert.ready": "Certificate issued",
  "domain.cert.until": (date: string) => `Certificate valid until ${date}`,
  "domain.cert.issuing": "cert-manager is issuing the certificate",
  "domain.cert.failed": "Certificate failed",
  "domain.cert.retries": "cert-manager retries on its own.",
  "domain.cert.says": "cert-manager says",
  "domain.cert.rateLimited":
    "Let's Encrypt has a limit on new certificates for this domain and turned this one down for now.",
  "domain.cert.rateLimitedShared":
    "Let's Encrypt has a limit on new certificates for sslip.io, shared by everyone who uses it, and turned this one down for now. For a site people rely on, use your own domain.",
  "domain.checkAgain": "Check again",
  "domain.checking": "Checking…",
  "pod.pods": "Replicas",
  "pod.named": (pod: string) => `Replica ${pod}`,
  "pod.none": "No replicas are running.",
  "pod.ready": "Ready",
  "pod.notReady": "Not ready",
  "pod.readyCount": (ready: number, total: number) => `${ready}/${total} ready`,
  "pod.restarts": (n: number) => plural(n, "restart", "restarts"),
  "pod.logsOf": (pod: string) => `Logs of ${pod}`,
  "resources.title": "Resources",
  "resources.hint":
    "Reserved amounts and maximums from the replica template. Usage needs metrics-server.",
  "resources.container": "Container",
  "resources.cpu": "CPU",
  "resources.memory": "Memory",
  "resources.requests": "Reserved",
  "resources.limits": "Maximum",
  "connections.title": "Connections",

  // Logs
  "logs.noPods": "No replica is running",
  "logs.noPodsBody": "Logs come from a running replica. They appear here once one starts.",
  "logs.podOf": (pod: string) => `Replica ${pod}`,
  "logs.replaced": (old: string, current: string) => `${old} was replaced. Showing ${current}.`,
  "logs.gone": (pod: string) =>
    `${pod} no longer exists. Looking for the replica that took its place.`,
  "logs.container": "Container",
  "logs.previous": "Previous",
  "logs.timestamps": "Timestamps",
  "logs.wrap": "Wrap lines",
  "logs.follow": "Follow",
  "logs.following": "Following",
  "logs.filter": "Filter lines",
  "logs.filterHint": "Start with - to leave lines out.",
  "logs.error": "Error",
  "logs.warn": "Warning",
  "logs.info": "Info",
  "logs.download": "Download",
  "logs.lines": "Log lines",
  "logs.empty": "This replica hasn't written anything yet.",
  "logs.noMatch": "No lines match.",
  "logs.jump": "Jump to latest",
  "logs.ended": "Stream ended",
  "logs.connecting": "Connecting…",
  "logs.resume": "Resume",
  "logs.refresh": "Refresh",
  "logs.count": (n: number) => plural(n, "line", "lines"),
  "logs.of": (n: number) => `of ${n}`,
  "logs.unavailable.no-previous-run": "No previous run: the container has not restarted.",
  "logs.unavailable.not-started": "No log yet: the container has not started.",
  "logs.unavailable.removed": "The cluster has removed this run's container and its log.",
  "logs.unavailableFix.no-previous-run": "Read the current run's log instead.",
  "logs.unavailableFix.not-started":
    "Wait for the container to start. The replica's events say what it is waiting for.",
  "logs.unavailableFix.removed":
    "Wait a moment and refresh. To keep logs past restarts, send them to a log store.",
  "logs.currentRun": "Show current run",
  "logs.history": "History",
  "logs.historyHint":
    "Lines every replica wrote, also replicas since replaced, from the log store.",
  "logs.historyLatest": (n: number) => `Latest ${n} lines`,
  "logs.historyNotInstalled": "Log history isn't turned on",
  "logs.historyNotInstalledBody":
    "Logs come from running replicas only. A cluster administrator turns history on with the Helm chart's proxy.enabled.",
  "logs.whatHistory": "Reading this project's log history",
  "logs.historyRoles":
    "Project developers have it, as does anyone with Kubernetes' view role in the namespace. Viewers read no logs.",
  "logs.historyUnavailable": "The log store did not respond.",
  "dock.show": "Show terminal",
  "dock.hide": "Hide terminal",
  "dock.resize": "Terminal height",
  "dock.close": (name: string) => `Close ${name}`,
  "dock.count": (count: number) => (count === 1 ? "1 terminal" : `${count} terminals`),
  "dock.emptyTitle": "No terminal open",
  "dock.emptyBody": "Open one from a service's Logs tab or More menu, or from search.",
  "dock.gone": "This service is no longer in the project.",
  "terminal.label": "Terminal",
  "terminal.noPodsBody":
    "A terminal opens inside a running replica. One can open once a replica starts.",
  "terminal.pod": "Replica",
  "terminal.connecting": "Connecting…",
  "terminal.open": "Connected",
  "terminal.reconnect": "Reconnect",
  "terminal.exited": (code: number) => `Exited with code ${code}`,
  "terminal.ended": "Ended",
  "terminal.noShell":
    "This container has no shell, as is common for slim images. A debug container adds one to this replica, beside the app.",
  "terminal.debugStarting": "Starting the debug container…",
  "terminal.debugEnded": "The debug container has ended. Open another to continue.",
  "terminal.debugOpen": "Connected to the debug container",
  "terminal.failed": "The container runtime ended the session.",
  "terminal.idle": "Closed after 15 minutes without input or output.",
  "terminal.lost": "Connection lost.",
  "terminal.rbac":
    "Opening a terminal needs permission to run commands in this project's containers. Send this to an admin. It grants only that permission, and only in this project:",
  "terminal.roleLabel": "Role and RoleBinding",
  "tools.about":
    "A terminal with kubectl and helm, acting as you in this project. It runs in a temporary container that Zeabur deletes once you're done.",
  "tools.localTitle": "Not available while Zeabur runs on your computer",
  "tools.yours": "Your kubectl terminals",
  "tools.starting": "Starting…",
  "tools.ready": "Ready",
  "tools.stopping": "Stopping",
  "tools.stopsAt": (when: string) => `Stops ${when}`,
  "tools.open": "Open",
  "tools.stop": "Stop",
  "tools.reopen": "Reopen",
  "clusterTools.about":
    "A terminal with kubectl and helm for the whole cluster, acting as you. kubectl starts in the default namespace. It runs in a temporary container that Zeabur deletes once you're done.",
  "clusterTools.tab": "cluster",
  "clusterTools.title": "Open a kubectl terminal for the cluster",
  "clusterTools.what": "Opening a kubectl terminal for the cluster",
  "clusterTools.starts": (namespace: string) =>
    `Starts a temporary container with kubectl and helm for you, in ${namespace}. kubectl starts in the default namespace and reaches every namespace you can.`,
  "nodeShell.action": "Node terminal",
  "nodeShell.actionOn": (node: string) => `Node terminal on ${node}`,
  "nodeShell.what": "Opening a node terminal",
  "nodeShell.pods": (node: string) => `Terminal replicas on ${node}`,
  "nodeShell.byYou": "opened by you",
  "nodeShell.by": (person: string) => `opened by ${person}`,
  "nodeShell.word.starting": "Starting",
  "nodeShell.word.stuck": "Not starting",
  "nodeShell.word.running": "Running",
  "nodeShell.word.deadline": "Ended at its time limit",
  "nodeShell.word.ended": "Ended",
  "nodeShell.openTitle": (node: string) => `Open a node terminal on ${node}`,
  "nodeShell.removeTitle": "Remove terminal replica",
  "nodeShell.open": "Open terminal",
  "nodeShell.remove": "Remove terminal replica",
  "nodeShell.removeLine": (pod: string, node: string) =>
    `Deletes ${pod} from ${node} right away. A terminal still open in it ends.`,
  "nodeShell.openedBy": (person: string) => `${person} opened it.`,
  "nodeShell.root": (node: string) =>
    `This is full root on ${node}'s host: every file, process and network interface. What you do there bypasses Zeabur and Kubernetes permissions, and nothing undoes it.`,
  "nodeShell.lasts": (minutes: number) =>
    `The terminal replica is removed when you close the terminal, and after ${minutes} minutes at most.`,
  "nodeShell.firstSession": (minutes: number) =>
    `If no terminal connects within ${minutes} minutes, the replica ends on its own.`,
  "nodeShell.recorded": "The node records an event with who opened it and when.",
  "nodeShell.exposedTitle": "Others could enter this terminal",
  "nodeShell.exposed": (namespace: string) =>
    `These role bindings let someone other than a cluster administrator create replicas or open terminals in ${namespace}. Remove them, then check again.`,
  "nodeShell.kubectl": "The same with kubectl",
  "nodeShell.write.create": "add",
  "nodeShell.write.patch": "change",
  "nodeShell.write.delete": "delete",
  "nodeShell.starting": (node: string) => `Starting the terminal replica on ${node}…`,
  "nodeShell.ended": "The session ended.",
  "nodeShell.left": "The session ended, but its terminal replica is still on the node.",
  "nodeShell.gone": "The session ended and its terminal replica was removed.",

  // Variables
  "vars.title": "Environment variables",
  "vars.about": "Settings the app reads when it starts, such as a database address or an API key.",
  "vars.emptyTitle": "No environment variables yet",
  "vars.emptyBody": "Add one to change a setting without building a new image.",
  "vars.direct": "Set directly",
  "vars.filter": "Filter variables",
  "vars.noMatch": "No variable matches.",
  "vars.also": "Also ",
  "vars.alsoObjects": (n: number) => `${n} from config files and secrets`,
  "vars.alsoCluster": (n: number) => `${n} filled in by Kubernetes`,
  "vars.fromObjects": "From config files and secrets",
  "vars.fromCluster": "Filled in by Kubernetes",
  "vars.reveal": (secret: string) => `Show values in ${secret}`,
  "vars.hide": (secret: string) => `Hide values in ${secret}`,
  "vars.secretsHint": "About secret values",
  "vars.revealHint":
    "Zeabur fetches a value only when you reveal it, and keeps it on this page only.",
  "vars.revealAll": "Show secret values",
  "vars.hideAll": "Hide secret values",
  "vars.lockedHint": "Your rights don't let you read secrets, so their values stay hidden.",
  "vars.view": "View",
  "vars.list": "List",
  "vars.names": (service: string) => `Points to the service ${service}`,
  "vars.atRuntime": "Set at runtime",
  "vars.allKeys": "All keys",
  "vars.keys": (n: number) => plural(n, "key", "keys"),

  // Networking
  "net.path": "Path",
  "net.publicTitle": "Public domains",
  "net.publicAbout": "Anyone on the internet can open these addresses.",
  "net.publicLead": (
    domains: number,
    service: string,
    ready: number,
    total: number,
    plain: number,
  ) =>
    `${domains === 1 ? "1 domain routes" : `${domains} domains all route`} to ${service}${total > 0 ? ` (${ready}/${total} ready)` : ""}${plain > 0 ? `; ${plain} without HTTPS` : ""}.`,
  "net.filter": "Filter domains",
  "net.noMatch": "No domain matches.",
  "net.noHttps": "No HTTPS",
  "net.https": "HTTPS",
  "net.routeMore": (host: string) => `Path, port and actions for ${host}`,
  "net.publicEmpty": (name: string) => `${name} has no public domain yet`,
  "net.publicEmptyBody": "Add a domain to open this service from a browser.",
  "net.publicNeedsPrivate":
    "A domain sends requests to the service's private address, so it needs one first.",
  "net.sendsTo": "Sends to",
  "net.privateTitle": "Private network",
  "net.privateAbout":
    "Other services in the cluster connect to it at these addresses. The internet can't reach them.",
  "net.privateEmpty": (name: string) => `${name} has no private address`,
  "net.privateEmptyBody":
    "Other services and its public domains can send it traffic only once it has one.",
  "net.port": (port: string) => `Port ${port}`,
  "net.anyPort": "The only port",
  "net.fullForm": "From another project",
  "net.forwardsTo": (port: string) => `Forwarded to the container's port ${port}`,
  "net.exposeHint": "To add one, apply a Kubernetes Service or run this command.",
  "net.noneTitle": (name: string) => `${name} can't receive connections`,
  "net.noneBody":
    "It has no private address and no public domain. A background service that only makes outgoing connections needs neither, and can still reach other services and the internet.",
  "net.scheduledTitle": (name: string) => `${name} doesn't receive traffic`,
  "net.scheduledBody":
    "A job runs and exits, so nothing connects to it. It can still connect to other services and the internet.",
  "reach.fromDomains": "Public domains",
  "net.anyHost": "Any host",
  "net.defaultBackend": "Default backend",

  // Storage
  "storage.title": "Volumes",
  "volume.fullestOf": (used: string, n: number) => `${used}, fullest of ${n} replicas`,
  "volume.full": (grows: boolean): string =>
    grows
      ? "When it is full, the app's writes fail. Free space in it, or set a larger size and apply."
      : "When it is full, the app's writes fail. Free space in it: its storage class can't grow it.",
  "volume.state.not-configured": "Volume use is not shown: there is no metrics store.",
  "volume.state.unreadable": "Volume use is not shown: reading the metrics store was refused.",
  "volume.state.unavailable": "Volume use is not shown: the metrics store did not respond.",
  "volume.state.not-collected":
    "Volume use is not shown: the metrics store has no kubelet volume metrics for this service.",
  "storage.about":
    "Files written to a volume stay when a replica restarts or moves to another node. Files anywhere else in the container are lost on restart.",
  "storage.emptyTitle": (name: string) => `${name} has no volume`,
  "storage.emptyBody": "Add one if the app must keep files, such as uploads or a database's data.",
  "storage.emptyJob": "To add one to a Job, edit its YAML.",
  "storage.bound": "Ready",
  "storage.pending": "Waiting for storage",
  "storage.lost": "Storage lost",
  "storage.mountPath": "Mount path",
  "storage.fromConfig": (name: string) => `Config file ${name}`,
  "storage.fromSecret": (name: string) => `Secret ${name}`,
  "storage.scratch": "Scratch space, emptied when the replica is replaced",
  "storage.fromCluster": "Provided by Kubernetes",
  "storage.otherMounts": "Other mounts",
  "storage.edit": "Edit",
  "storage.readOnly": "Read-only",
  "storage.delete": (path: string) => `Delete volume ${path}`,
  "storage.deleteSaid": (service: string, path: string, claim: string) =>
    `${service} stops mounting ${path}, and the volume (${claim}) is deleted with its data.`,
  "storage.writable": "Read and write",
  "storage.status": "Status",
  "storage.newPath": "Mount path of the new volume",
  "storage.newVolume": "the new volume",
  "storage.sizeOf": (path: string) => `Size of ${path}`,
  "storage.pathOf": (path: string) => `Mount path of ${path}`,
  "storage.readOnlyOf": (path: string) => `${path} read-only`,
  "storage.blocked": (n: number) => `Fix ${plural(n, "volume", "volumes")} to save.`,
  "storage.problem.badPath": "Enter an absolute path, such as /data.",
  "storage.problem.pathTaken": "Another mount uses this path.",
  "storage.problem.badSize": "Enter a size such as 1 GiB.",
  "storage.problem.shrinks": "A volume can grow but never shrink.",
  "storage.problem.badName": "Use lowercase letters, digits and -.",
  "storage.problem.nameTaken": "Another new volume has this name.",
  "storage.problem.badClaimName": "Use lowercase letters, digits, - and ., at most 253 characters.",
  "storage.class": "Storage class",
  "storage.kind.local": "Local disk",
  "storage.kind.replicated": "Replicated disk",
  "storage.kind.other": (provisioner: string) => `Disk by ${provisioner}`,
  "storage.option": (kind: string, name: string, isDefault: boolean) =>
    `${kind} (${name}${isDefault ? ", default" : ""})`,
  "storage.about.local": "Fastest, but kept on one node: replicas that use it run only there.",
  "storage.about.replicated": "Copied to several nodes, so it moves with its replicas.",
  "storage.about.other": "How it behaves is up to its driver.",
  "storage.container": "Mount in container",
  "storage.grows": "It can grow later, not shrink.",
  "storage.fixedSize": "It can't grow later, so choose the size it needs.",
  "storage.growUnknown": "Whether it can grow later is up to the cluster's storage class.",
  "storage.selfReplicating": (engine: string) =>
    `${engine} copies its data to its other instances itself, so local disks suit it.`,
  "storage.choosing": "Choosing a storage class",
  "storage.onDefault": "New volumes go on the cluster's default storage class.",
  "storage.on.node": (node: string) => `Kept on node ${node}`,
  "storage.on.pinned": "Kept on some nodes only",
  "storage.on.firstUse": "Kept on the node its first replica runs on",
  "storage.on.moves": "Moves between nodes",
  "storage.canGrow": "Can grow",
  "storage.cannotGrow": "Can't grow",
  "storage.fixed": (name: string, elsewhere: boolean) =>
    elsewhere
      ? `${name} can't grow a volume. For more room, copy the data to a new volume on a storage class that can grow, then remove this one.`
      : `${name} can't grow a volume, and no storage class in this cluster can. For more room, ask a cluster administrator for one that can, such as Longhorn.`,
  "storage.reclaim.Delete": "Deleting the volume deletes its data",
  "storage.reclaim.Retain": "Data stays after the volume is deleted",
  "storage.binding.WaitForFirstConsumer": "Made where its first replica runs",
  "storage.binding.Immediate": "Made as soon as the volume is added",

  // Palette
  "palette.search": "Jump to a project, service, replica, or domain",
  "palette.recent": "Recent",
  "palette.actions": "Actions",
  "palette.openLogs": (name: string) => `Open logs of ${name}`,
  "palette.openTerminal": (name: string) => `Open a terminal in ${name}`,
  "palette.openVariables": (name: string) => `Edit variables of ${name}`,
  "palette.openChanges": (name: string) => `See deployments of ${name}`,
  "palette.toggleTheme": "Switch theme",
  "palette.components": "Components",
  "palette.empty": "Nothing found.",
  "palette.in": (what: string, namespace: string) => `${what} in ${namespace}`,
  "palette.move": "move",
  "palette.open": "open",

  // Settings
  "settings.title": "Settings",
  "settings.preferences": "Preferences",
  "settings.languageHint": "Messages from the cluster stay in English.",
  "settings.theme": "Theme",
  "settings.about": "About",
  "settings.user": "Signed in as",
  "settings.groups": "Groups",
  "settings.kubernetesVersion": "Kubernetes",
  "settings.catalogueCommit": "Zeabur Catalog",
  "settings.components": "Components",
  "settings.filter": "Filter",
  "settings.searchComponents": "Search components",
  "settings.installed": "Installed",
  "settings.recommended": "Recommended",
  "settings.optional": "Optional",
  "settings.unsupported": "Not supported",
  "settings.installCommand": "Install",
  "settings.homepage": "Home page",
  "settings.opensInNewTab": "(opens in a new tab)",
  "category.traffic": "Traffic",
  "category.observability": "Observability",
  "category.scaling": "Scaling",
  "category.security": "Security",
  "category.data": "Data",
  "category.operations": "Operations",
  "category.other": "Other",
  "cap.certificates": "Certificates",
  "cap.config-reload": "Restart on config change",
  "cap.container-registry": "Container registry",
  "cap.databases": "Databases",
  "cap.event-autoscaling": "Event autoscaling",
  "cap.image-updates": "Image updates",
  "cap.ingress": "Ingress",
  "cap.intrusion-prevention": "Intrusion prevention",
  "cap.logs": "Log storage",
  "cap.metrics": "Metrics",
  "cap.node-updates": "Node updates",
  "cap.object-storage": "Object storage",
  "cap.observability-operator": "Monitoring operator",
  "cap.postgres": "PostgreSQL databases",
  "cap.remote-access": "Remote access",
  "cap.replicated-storage": "Replicated storage",
  "cap.resource-recommendations": "Resource recommendations",
  "cap.runtime-security": "Runtime security",
  "cap.uptime-probes": "Uptime probes",
  "cap.volume-backup": "Volume backups",
  "cap.vulnerability-scanning": "Vulnerability scanning",
  // Objects and YAML
  "project.entry.objects": "Objects",
  // A project's namespace
  "project.entry.settings": "Settings",
  "action.adopt": "Use this namespace",
  "action.unadopt": "Stop showing as a project",
  "action.deleteProject": "Delete project",
  "action.openDebugTerminal": "Open debug terminal",
  "action.openToolsTerminal": "Open kubectl terminal",
  "action.saveDetails": "Save details",
  "title.adopt": (ns: string) => `Use the existing namespace ${ns}`,
  "title.unadopt": (ns: string) => `Stop showing ${ns} as a project`,
  "title.deleteProject": (ns: string) => `Delete project ${ns}`,
  "title.details": (ns: string) => `${ns} details`,
  "title.pauseAll": (ns: string) => `Pause every service in ${ns}`,
  "title.restartAll": (ns: string) => `Restart every service in ${ns}`,
  "confirm.createProject":
    "Creates the namespace with the project label; services in it may run as root but can't reach the host (Pod Security baseline). Nothing else changes.",
  "confirm.adopt": "Adds the project label to the namespace. Nothing in it changes.",
  "confirm.unadopt": "The namespace stops showing as a project. Nothing in it is deleted.",
  "confirm.deleteProject":
    "Deletes the project's namespace and everything in it, as kubectl delete namespace does. This can't be undone.",
  "confirm.debugContainer": (pod: string) =>
    `Adds a debug container with a shell to replica ${pod}. It can see the app's processes.`,
  "confirm.debugStays":
    "It stays in the replica until the replica is replaced, for example by a restart. Kubernetes cannot remove it sooner.",
  "confirm.debugReused":
    "A debug container already runs in this replica; the terminal opens in it.",
  "confirm.clusterTools":
    "Starts a temporary container with kubectl and helm for you, where only Zeabur and cluster administrators can reach it. kubectl starts in this project.",
  "confirm.toolsActsAs": (person: string) =>
    `kubectl and helm act as ${person}, with your own permissions.`,
  "confirm.toolsCredential": (minutes: number) =>
    `Your credential in it runs out in ${plural(minutes, "minute", "minutes")}; reopen the terminal for a fresh one.`,
  "confirm.toolsStops": (most: number, after: number) =>
    `It stops after ${plural(most, "minute", "minutes")} at most, and is deleted ${plural(after, "minute", "minutes")} after its last terminal closes.`,
  "confirm.toolsReused": "Your kubectl terminal is already running; it opens again.",
  "confirm.labelVolume": (claim: string) =>
    `Adds a label and an annotation to ${claim} so that only its own schedule backs it up. Nothing restarts.`,
  "confirm.details": "Saves the name, description, and icon on the namespace.",
  "access.title": (ns: string) => `You can't change ${ns}`,
  "access.you": "<your user name>",
  "access.ask": (ns: string) =>
    `Ask a cluster administrator for permission to patch the namespace ${ns}.`,
  "access.askManage": (ns: string) =>
    `Ask a cluster administrator for permission to patch the namespace ${ns} and to zeabur-manage it. Without zeabur-manage, Zeabur's admission policy lets a patch change only the project's name, description and icon.`,
  "access.detailsOnly":
    "With this permission you can change the project's name, description and icon. Zeabur's admission policy keeps the namespace's labels as they are.",
  "access.alsoLabels":
    "This permission also lets you change the namespace's other labels, including its Pod Security levels (pod-security.kubernetes.io/*). Kubernetes can't limit it to one label.",
  "access.role": "A ClusterRole limited to this namespace with resourceNames:",
  "access.roleLabel": "ClusterRole and binding",
  "access.kubectl": "Or an admin can run:",
  "access.kubectlLabel": "kubectl command",
  "projectSettings.description": "Description",
  "projectSettings.icon": "Icon",
  "projectSettings.tooLong": (max: number) => `Use at most ${max} characters.`,
  "projectSettings.readOnly": "You can see these but not change them.",
  "projectSettings.getAccess": "How to get access",
  "projectSettings.system":
    "Kubernetes keeps this namespace for its own components, so it can't be a project.",
  "projectSettings.title": "Project settings",
  "projectSettings.back": "Back",
  "projectSettings.general": "General",
  "projectSettings.basic": "Basic information",
  "projectSettings.iconHint": "Shown beside the project's name.",
  "projectSettings.changeIcon": "Change icon",
  "projectSettings.name": "Project name",
  "projectSettings.nameHint": (ns: string) =>
    `Shown in place of ${ns}. The namespace's own name can't change.`,
  "projectSettings.descriptionHint": "What the project is for, shown under its name.",
  "projectSettings.namespaceGroup": "Kubernetes namespace",
  "projectSettings.namespace": "Namespace",
  "projectSettings.namespaceHint": "The project is this namespace. Its name can't change.",
  "projectSettings.shown": "Shown as a project",
  "projectSettings.shownNo":
    "No. Showing it as a project only adds Zeabur's project label; nothing else changes.",
  "network.title": "Network",
  "network.isolation": "Other projects",
  "network.isolated":
    "Can't reach this project's services. Services in this project and Zeabur's components still can.",
  "network.open": "Any replica in the cluster can reach this project's services.",
  "network.isolate": "Isolate",
  "network.allowedHint": (policy: string) =>
    `Can reach every service of this project (NetworkPolicy ${policy}).`,
  "network.disallow": "Remove",
  "network.disallowTitle": (from: string, project: string) =>
    `Stop letting ${from} reach ${project}`,
  "network.allow": "Let a project in",
  "network.allowHint": "It can then reach every service of this project.",
  "network.noOthers": "No other project to let in.",
  "network.allowAction": "Allow",
  "network.allowTitle": (from: string, project: string) => `Let ${from} reach ${project}`,
  "projectSettings.batch": "Batch actions",
  "projectSettings.pauseAll": "Pause all services",
  "projectSettings.pauseAllHint":
    "Stops every service in this project; you can bring them back later.",
  "projectSettings.pause": "Pause",
  "projectSettings.restartAll": "Restart all services",
  "projectSettings.restartAllHint":
    "Replaces the replicas of every service that can restart, a few at a time.",
  "projectSettings.danger": "Danger zone",
  "projectSettings.remove": "Remove from Zeabur",
  "projectSettings.removeHint":
    "Removes only Zeabur's project label. The namespace and everything in it stay.",
  "projectSettings.removeAction": "Remove",
  "projectSettings.delete": "Delete project",
  "projectSettings.deleteHint":
    "Deletes the namespace and everything in it. You see what goes and what stays before you confirm.",
  "projectSettings.deleteAction": "Delete",
  "icon.folder": "Folder",
  "icon.globe": "Globe",
  "icon.storefront": "Shop",
  "icon.rocket": "Rocket",
  "icon.cube": "Cube",
  "icon.database": "Database",
  "icon.cloud": "Cloud",
  "icon.code": "Code",
  "icon.terminal": "Terminal",
  "icon.chart": "Chart",
  "icon.chat": "Chat",
  "icon.book": "Book",
  "icon.flask": "Flask",
  "icon.game": "Game",
  "icon.heart": "Heart",
  "icon.briefcase": "Briefcase",
  "objects.filter": "Filter objects",
  "objects.generated": "Generated",
  "objects.count": (n: number) => plural(n, "object", "objects"),
  "objects.none": "This project has no objects.",
  "objects.noMatch": (query: string) => `No objects match “${query}”.`,
  "objects.owner": (kind: string) => `by ${kind}`,
  "objects.empty": "Empty",
  "objects.scanning": "Scanning…",
  "objects.refresh": "Refresh",
  "objects.denied": "No access",
  "objects.pin": (kind: string) => `Pin ${kind}`,
  "objects.unpin": (kind: string) => `Unpin ${kind}`,
  "object.conditions": "Conditions",
  "object.status": "Status",
  "object.noStatus": "This object reports no status.",
  "object.moreStatus": (fields: string) => `Other fields: ${fields}`,
  "object.statusLabel": (name: string) => `Status of ${name}`,
  "create.button": "Create",
  "create.title": (project: string) => `Create in ${project}`,
  "create.palette": (project: string) => `Create in ${project}…`,
  "create.templateAdd": "Insert example",
  "create.common": "Common kinds",
  "create.custom": "Custom resources",
  "create.more": "Other kinds you may create",
  "create.documents": (n: number) => plural(n, "document", "documents"),
  "create.editor": "YAML to create",
  "create.review": "Review changes",
  "create.empty": "Write the objects as YAML, or insert an example.",
  "create.inOrder": "Created in this order",
  "create.inOrderHint":
    "Config files, secrets, disks and service accounts first, then Services, then workloads, then the rest, so what a workload reads exists when it starts.",
  "create.docLine": (document: number, line: number) => `Document ${document}, line ${line}`,
  "create.docLineField": (document: number, line: number, field: string) =>
    `Document ${document}, line ${line}, ${field}`,
  "create.showInYaml": "Show in YAML",
  "create.problems": (n: number) => plural(n, "problem", "problems"),
  "create.notes": (n: number) => plural(n, "note", "notes"),
  "create.fixFirst": "Fix the problems above to review.",
  "create.found.invalidYaml": "This line doesn't read as YAML: check its indentation and quotes.",
  "create.found.missingField": (field: string) => `Every object needs ${field}.`,
  "create.found.clusterScoped": (kind: string) =>
    `${kind} belongs to the whole cluster. Zeabur creates objects only inside a project.`,
  "create.found.notServed": (kind: string, apiVersion: string) =>
    `This cluster doesn't serve ${kind} (${apiVersion}).`,
  "create.found.rights": (kind: string, project: string) =>
    `Your rights don't seem to allow creating ${kind} in ${project}. The review asks the cluster.`,
  "create.found.otherNamespace": (namespace: string, project: string) =>
    `Namespace ${namespace} isn't this project. Remove the line, or write ${project}.`,
  "create.found.duplicate": (kind: string, name: string) => `${kind} ${name} is written twice.`,
  "create.found.exists": (kind: string, name: string) =>
    `A ${kind} named ${name} already exists. Create never changes it.`,
  "create.edit": (name: string) => `Edit ${name}`,
  "create.planOne": (kind: string, name: string) => `Create ${kind} ${name}`,
  "create.planMany": (n: number, project: string) => `Create ${n} objects in ${project}`,
  "create.createsMany": (n: number, list: string) =>
    `Creates ${n} objects, one after another: ${list}.`,
  "create.noTransaction":
    "Kubernetes has no undo across objects: if one is refused, the ones before it stay, and you can delete exactly those.",
  "create.buttonMany": (n: number) => `Create ${n} objects`,
  "create.partialTitle": (done: number, n: number) => `${done} of ${n} created`,
  "create.notAttempted": "not attempted",
  "create.deleteCreated": (n: number) => `Delete the ${n} just created`,
  "create.keep": "Keep them",
  "create.cleanupTitle": (n: number) =>
    n === 1 ? "Delete the object just created" : `Delete the ${n} objects just created`,
  "tab.yaml": "YAML",
  "yaml.hidden": "Secret values are hidden.",
  "yaml.intro": (name: string) =>
    `This is how ${name} is set up in the cluster now. A change is written as you make it.`,
  "yaml.label": (name: string) => `YAML of ${name}`,

  // Plans
  "plan.planning": "Checking changes…",
  "plan.apply": "Apply",
  "plan.applying": "Applying…",
  "plan.cancel": "Cancel",
  "plan.back": "Back",
  "plan.replan": "Review again",
  "plan.replanned": "Something changed after you reviewed this. Please check it again.",
  "plan.replannedAdded": (names: string) => `New this time: ${names}`,
  "plan.replannedDropped": (names: string) => `No longer included: ${names}`,
  "plan.details": "Technical details",
  "plan.expiresIn": (time: string) => `Expires in ${time}`,
  "plan.expired": "Expired",
  "plan.warnings": "Before you apply",
  "plan.owners": "Fields owned by",
  "plan.when": (tool: string) =>
    tool === "Helm"
      ? "the next Helm upgrade"
      : tool === "kubectl"
        ? "the next kubectl apply"
        : `the next ${tool} sync`,
  "plan.toolSkipped": (names: string, count: number, tool: string, when: string) =>
    `Leaves out ${names}: ${tool} manages ${count === 1 ? "it" : "them"}, and ${when} would undo the change.`,
  "plan.toolPutBack": (when: string, paths: string, object: string) =>
    `${when.charAt(0).toUpperCase()}${when.slice(1)} puts back ${paths} on ${object}.`,
  "plan.diff": "YAML diff",
  "plan.create": "New",
  "plan.delete": "Deleted",
  "plan.typeName": (name: string) => `Type ${name} to confirm`,
  "plan.fillName": "Fill in the name",
  "plan.added": "added",
  "plan.removed": "removed",
  "plan.changed": "changed",
  "plan.exists": "Exists",
  "plan.hidden": "hidden",
  "plan.filter": "Filter changes",
  "plan.noMatch": "No change matches.",
  "plan.showAll": (n: number) => `Show all ${n}`,
  "plan.alsoRestart": (count: number): string =>
    count === 1 ? "Restart it too" : "Restart them too",
  "plan.restartByItself": "Restart automatically next time",
  "plan.other": "Other fields",
  "plan.field.size": "Size",
  "plan.col.field": "Field",
  "plan.col.before": "Before",
  "plan.col.after": "After",
  "plan.col.service": "Service",
  "plan.col.replicas": "Replicas",
  "plan.col.effect": "Effect",
  "plan.short.title": "Not enough room",
  "plan.short.nodes": (names: string, n: number) =>
    n === 0
      ? "The new replicas wait as Pending until a node has room. The old ones keep running, so the service stays up, but the change isn't finished until then."
      : `The new replicas wait as Pending until a node has room. ${names} ${n === 1 ? "doesn't" : "don't"} run until then.`,
  "plan.short.quota": (names: string, n: number) =>
    n === 0
      ? "The quota refuses the new replicas until it has room. The old ones keep running, so the service stays up, but the change isn't finished until then."
      : `The quota refuses the new replicas until it has room. ${names} ${n === 1 ? "doesn't" : "don't"} run until then.`,
  "plan.short.fix": (quota: boolean, nodes: boolean): string =>
    quota && nodes
      ? "To finish it, raise the quota and free up room on a node, or reserve less per replica."
      : quota
        ? "To finish it, raise the quota or reserve less per replica."
        : "To finish it, free up room on a node or reserve less per replica.",
  "plan.fact.removed": (n: number) => `${n} removed`,
  "plan.fact.changed": (n: number) => `${n} changed`,
  "plan.fact.added": (n: number) => `${n} added`,
  "plan.fact.replicas": (from: number, to: number) =>
    `${from} → ${plural(to, "replica", "replicas")}`,
  "plan.fact.staysUp": "stays up",
  "plan.fact.downBriefly": "down briefly",
  "plan.fact.stops": "stops serving",
  "plan.fact.fits": "fits",
  "plan.outcome.stops.label": "Stop serving",
  "plan.outcome.stops.count": (n: number) => `${n} stop serving`,
  "plan.outcome.down.label": "Down while restarting",
  "plan.outcome.down.count": (n: number) => `${n} down briefly`,
  "plan.outcome.notReady.label": "Not ready now",
  "plan.outcome.notReady.count": (n: number) => `${n} not ready now`,
  "plan.outcome.single.label": "One replica, may be down a few seconds",
  "plan.outcome.single.count": (n: number) => `${n} on one replica`,
  "plan.outcome.starts.label": "Start",
  "plan.outcome.starts.count": (n: number) => `${n} start`,
  "plan.outcome.up.label": "Stay up",
  "plan.outcome.up.count": (n: number) => `${n} stay up`,
  "plan.effect.recreate": "Stops every replica, then starts new ones",
  "plan.effect.onDelete": "Waits until its replicas are deleted",
  "plan.effect.dataStays": "Data stays on its volume",
  "plan.cost.dataStaysOne": (name: string, paths: string) =>
    `The data of ${name} in ${paths} stays on its volume.`,
  "plan.cost.dataStays": (n: number) =>
    n === 1
      ? "Data on the volume stays."
      : `Data stays on the volumes of the ${n} services that have one.`,
  "plan.cost.undo": "Once applied, Undo in the notification scales back.",
  "risk.fewPods": (names: string, n: number) =>
    `${names} ${n === 1 ? "has" : "have"} fewer than two ready replicas, so ${n === 1 ? "it" : "each"} may be down for a few seconds.`,
  "risk.scaleToZero": (names: string, n: number) =>
    `${names} ${n === 1 ? "stops" : "stop"}: nothing serves ${n === 1 ? "it" : "them"} until scaled up again.`,
  "risk.deletesKeys": "A deleted value can't be restored here.",
  "risk.rollback": "Template changes made after that revision are undone.",
  "risk.storage": "A volume can't shrink, and a new or moved mount replaces the replicas.",
  "risk.removesDomain": "Requests to a removed domain no longer reach this project.",
  "risk.opensPort": "Anyone on the internet can connect to it, without a password from Zeabur.",
  "risk.deletes": "Deleting can't be undone.",
  "warn.rightsMissing": (rights: string) => `The cluster may refuse this: you lack ${rights}.`,
  "warn.recreated": (names: string, tool: string) =>
    `${tool} manages ${names}: it may create them again after the deletion`,
  "warn.restartNeeded": (names: string) => `Takes effect after a restart: ${names}`,
  "warn.reloaderRestarts": (names: string) => `Reloader restarts ${names}`,
  "warn.reloaderMayRestart": (names: string) =>
    `Reloader restarts ${names}, if it runs in this cluster`,
  "warn.tlsHostChanged": (host: string, route: string) =>
    `${route} lists the old host for HTTPS; ${host} needs a new certificate`,
  "warn.httpsCertificate": (host: string, issuer: string) =>
    `${host} uses HTTPS: cert-manager (${issuer}) issues its certificate after you apply`,
  "warn.httpOnlyNoIssuer": (host: string) =>
    `${host} is HTTP only. An administrator can name a cert-manager issuer to make new domains HTTPS`,
  "warn.httpOnlyElsewhere": (host: string, route: string) =>
    `${host} is HTTP only: ${route}'s certificates don't come from cert-manager, so add ${host} where they're made`,
  "warn.secretValuesUnknown": (name: string) =>
    `Unable to read ${name}'s values: the diff shows keys only`,
  "warn.configNotRolledBack": (workload: string) =>
    `${workload} reads config files or secrets: a rollback leaves their contents as they are`,
  "warn.storageClassUnknown": (claim: string) =>
    `Unable to read the volume's StorageClass (${claim}): the cluster decides whether it can grow`,
  "warn.readWriteOnceShared": (workload: string) =>
    `${workload} runs several replicas: a ReadWriteOnce volume mounts on one node only`,
  "warn.readWriteOncePodShared": (workload: string) =>
    `${workload} runs several replicas: a ReadWriteOncePod volume serves one replica at a time`,
  "warn.deletesDependents": (owner: string, names: string) =>
    `Also deletes what ${owner} owns: ${names}`,
  "warn.deletesOwned": (owner: string, names: string) =>
    `Also deletes what ${owner} made and nothing else uses: ${names}`,
  "warn.staysShared": (object: string, names: string) => `${object} stays: ${names} also use it`,
  "warn.staysRoute": (object: string) => `${object} stays: it also leads to a Service that stays`,
  "warn.staysControlled": (object: string, owner: string) =>
    `${object} stays: it belongs to ${owner}`,
  "warn.staysManaged": (object: string, tool: string) => `${object} stays: ${tool} manages it`,
  "warn.staysNotMade": (object: string) =>
    `${object} stays: the service uses it but was not created with it`,
  "warn.podReplaced": (pod: string, owner: string) =>
    `${owner} starts a new replica in place of ${pod}`,
  "warn.claimDataDeleted": (claim: string, volume: string) =>
    `The volume's data is deleted (${claim}): its PersistentVolume ${volume} has reclaim policy Delete`,
  "warn.claimDataRetained": (claim: string, volume: string) =>
    `The volume's data stays (${claim}): PersistentVolume ${volume} keeps it (reclaim policy Retain) until an admin removes it`,
  "warn.claimDataUnlessRetained": (claim: string) =>
    `The volume's data is deleted with it (${claim}); if the cluster is set to keep it, an admin can still recover it`,
  "warn.stillUsed": (name: string, names: string) =>
    `${names} still read ${name}: their new replicas will not start without it`,
  "warn.claimStillMounted": (name: string, names: string) =>
    `${names} still mount the volume (${name}): Kubernetes removes it once no replica uses it`,
  "warn.ingressOpened": (names: string) =>
    `No longer isolated: ${names} accept traffic from anywhere`,
  "warn.egressOpened": (names: string) => `No longer limited: ${names} can send traffic anywhere`,
  "warn.connectionsOpened": (pairs: string) => `Allowed again: ${pairs}`,
  // Confirmations
  "confirm.restartSingle": (name: string) =>
    `${name} has one replica, so it may be down for a few seconds while it restarts.`,
  "confirm.restartNone": (name: string) =>
    `No replica of ${name} is ready now; restarting replaces them all.`,
  "confirm.rolling": (name: string, pods: number) =>
    `The ${pods} replicas of ${name} are replaced one at a time, so the service stays up.`,
  "confirm.rollingSingle": (name: string) =>
    `${name} has one replica, so it may be down for a few seconds while it is replaced.`,
  "confirm.rollingNone": (name: string) => `${name}'s replicas are replaced.`,
  "confirm.env": (removed: string, added: string, changed: string, service: string) =>
    `${sentenceCase(
      new Intl.ListFormat("en").format(
        [
          removed && `remove ${removed}`,
          changed && `change ${changed}`,
          added && `add ${added}`,
        ].filter(Boolean),
      ),
    )} on ${service}.`,
  "confirm.data": (removed: string, added: string, changed: string, kind: string, name: string) =>
    `${sentenceCase(
      new Intl.ListFormat("en").format(
        [
          removed && `delete ${removed}`,
          changed && `change ${changed}`,
          added && `add ${added}`,
        ].filter(Boolean),
      ),
    )} in ${kind} ${name}.${removed ? " A deleted value can't be restored here." : ""}`,
  "confirm.keepsOld": (names: string, count: number) =>
    `${names} ${count === 1 ? "keeps" : "keep"} the old value until restarted.`,
  "confirm.readersKeepOld": (names: string, object: string, count: number) =>
    `${names} ${count === 1 ? "reads" : "read"} ${object} and ${count === 1 ? "keeps" : "keep"} the old value until restarted.`,
  "confirm.reloaderRestarts": (names: string, count: number) =>
    `Reloader then restarts ${names} with the new ${count === 1 ? "value" : "values"}.`,
  "confirm.reloaderMayRestart": (names: string, count: number) =>
    `${names} ${count === 1 ? "is" : "are"} set to restart with Reloader; Zeabur can't see whether it runs in this cluster.`,
  "confirm.configReloadOn": (name: string) =>
    `${name} restarts when a config file or secret it reads changes. Nothing restarts now.`,
  "confirm.securityRestricted": (name: string) =>
    `${name} becomes Strict: it runs as a non-root user and can't gain privileges. Its replicas are replaced.`,
  "confirm.securityBaseline": (name: string) =>
    `${name} becomes Compatible: it may run as root. Its replicas are replaced.`,
  "title.security": (name: string) => `Security of ${name}`,
  "security.hint":
    "What the service's containers may do. Compatible runs any image; Strict suits an image that runs as a numeric non-root user.",
  "security.profile.restricted": "Strict",
  "security.profile.baseline": "Compatible",
  "security.profile.privileged": "Unrestricted",
  "security.about.restricted":
    "Runs as a non-root user and can't gain privileges. An image that runs as root, such as nginx:latest, won't start; use an unprivileged one, such as nginxinc/nginx-unprivileged.",
  "security.aboutBuilt":
    "Runs as a non-root user and can't gain privileges. Railpack builds run as user 1000; a Dockerfile needs a numeric user other than root, such as USER 1000.",
  "security.about.baseline": "May run as root, but still can't reach the host or gain privileges.",
  "security.about.privileged":
    "Its settings ask for more than Compatible allows, such as reaching the host; they were set outside Zeabur. Edit its YAML to change them.",
  "security.picked.restricted": (user: string) =>
    `The image runs as user ${user}, so Zeabur chose Strict.`,
  "security.picked.baseline":
    "The image runs as root or names its user, so Zeabur chose Compatible.",
  "security.projectRestricted":
    "This project allows only Strict services, so a Compatible one won't start. Ask an admin to set the project's Pod Security level to baseline.",
  "security.kube.service": "Service meets",
  "security.kube.project": "Project enforces",
  "security.kube.unset": "No label: the cluster's default",
  "security.kube.strict": "Strict sets",
  "confirm.configReloadOff": (name: string) =>
    `${name} no longer restarts when its config files or secrets change.`,
  "confirm.alsoReads": (names: string, object: string, count: number) =>
    `Also affects ${names} (${count === 1 ? "it reads" : "they read"} ${object}).`,
  "confirm.alsoAffects": (names: string) => `Also affects ${names}.`,
  "confirm.addDomain": (host: string, service: string) => `Serve ${service} at ${host}.`,
  "confirm.image": (service: string, tag: string) => `${service} will run ${tag}.`,
  "confirm.rebuild": (build: string) =>
    `Zeabur Builder builds ${build} again from the newest commit of its branch.`,
  "confirm.buildOptions": (build: string) =>
    `Zeabur Builder builds ${build} with these options from the next build.`,
  "confirm.uploadToken": (build: string) =>
    `Zeabur Builder lets you upload a folder to ${build} for 15 minutes.`,
  "confirm.archiveDigest": (build: string) =>
    `Zeabur Builder builds ${build} from the folder you uploaded.`,
  "confirm.autoUpdateOn": (service: string) =>
    `Each new build of ${service} will go live as soon as it finishes.`,
  "confirm.autoUpdateOff": (service: string) =>
    `New builds of ${service} will wait for you to deploy them.`,
  "confirm.renameDomain": (from: string) => `Links to ${from} stop working.`,
  "confirm.removeDomain": (host: string) => `Requests to ${host} no longer reach this project.`,
  "confirm.rollback": (revision: number) =>
    `Template changes made after revision ${revision} are undone.`,
  "confirm.notRolledBack": "A rollback leaves config file and secret contents as they are.",
  "confirm.noShrink": "A volume can't shrink after it grows.",
  "confirm.addVolume": (size: string, service: string) => `${service} gets a new ${size} volume.`,
  "confirm.addVolumeIn": (size: string, service: string, container: string) =>
    `${service} gets a new ${size} volume, mounted in its container ${container}.`,
  "confirm.moveMount": (service: string, from: string, to: string) =>
    `Move ${service}'s mount from ${from} to ${to}.`,
  "confirm.readOnly": (path: string, service: string) => `Make ${path} read-only in ${service}.`,
  "confirm.writable": (path: string, service: string) => `Make ${path} writable in ${service}.`,
  "confirm.mount": (service: string) => `Change a mount of ${service}.`,
  "confirm.removeVolume": (service: string, path: string) =>
    `${service} stops mounting ${path}. The volume and the data on it stay; to free the space, delete its disk under Objects.`,
  "confirm.swapClaim": (service: string, from: string, to: string) =>
    `${service} stops, then starts again with volume ${to} in place of ${from}. It is down for a short time.`,
  "confirm.probeSet.readiness": (check: string, period: number, _failures: number) =>
    `A replica gets no traffic until ${check} succeeds (checked every ${period} s).`,
  "confirm.probeSet.liveness": (check: string, period: number, failures: number) =>
    `If ${check} fails ${failures} times in a row, the cluster restarts the container (checked every ${period} s).`,
  "confirm.probeSet.startup": (check: string, period: number, failures: number) =>
    `Other checks wait until ${check} succeeds; after ${failures} failures the cluster restarts the container (checked every ${period} s).`,
  "confirm.probeRemoved.readiness": "Without it, a replica gets traffic as soon as it starts.",
  "confirm.probeRemoved.liveness":
    "Without it, the cluster doesn't restart a container that hangs.",
  "confirm.probeRemoved.startup":
    "Without it, the other checks start as soon as the container does.",
  "confirm.check.http": (port: string, path: string) => `GET ${path} on port ${port}`,
  "confirm.check.tcp": (port: string) => `connecting to port ${port}`,
  "confirm.check.grpc": (port: string) => `the gRPC health check on port ${port}`,
  "confirm.check.exec": (command: string) => `running ${command}`,
  "confirm.scale": (name: string, from: number, to: number) =>
    `Scale ${name} from ${from} to ${to} ${to === 1 ? "replica" : "replicas"}.`,
  "confirm.scaleTo": (name: string, to: number) =>
    `Scale ${name} to ${to} ${to === 1 ? "replica" : "replicas"}.`,
  "confirm.scaleZero": (name: string) =>
    `${name} stops: no replicas run until you scale it up again.`,
  "confirm.resources": (pairs: [string, string][]) =>
    `${pairs.map(([label, value]) => `${label} ${value}`).join(", ")}.`,
  "confirm.yamlAsIs":
    "Zeabur writes the YAML as it is: every field you changed takes effect at once.",
  "confirm.create": (kind: string, name: string) => `Create ${kind} ${name}.`,
  "confirm.scheduleTask": (name: string, service: string) =>
    `Add CronJob ${name}: it runs the command in a new container copied from ${service}. ${service} doesn't restart.`,
  "confirm.runTask": (name: string) =>
    `Start one run of ${name} now, as a new Job. Its schedule doesn't change.`,
  "confirm.editTask": (name: string) =>
    `Change when and how ${name} runs, from its next run. A run already going keeps going.`,
  "confirm.pauseTask": (name: string) =>
    `Stop running ${name} on schedule. A run already going keeps going, and Run now still works.`,
  "confirm.resumeTask": (name: string) => `Run ${name} on schedule again.`,
  "confirm.redirect": (host: string, to: string) => `Send visitors of ${host} to ${to}.`,
  "confirm.redirectOff": (host: string) => `Stop redirecting ${host}.`,
  "confirm.envImport": (kind: string, name: string, service: string) =>
    `${service} reads every key of ${kind} ${name} as variables. Its replicas restart.`,
  "confirm.publicPort": (port: number, name: string) =>
    `Open port ${port} of ${name} to the internet.`,
  "confirm.publicPortOff": (port: number, name: string) =>
    `Close port ${port} of ${name}. Clients outside the cluster can no longer connect to it.`,
  "confirm.envImportOff": (kind: string, name: string, service: string) =>
    `${service} stops reading the keys of ${kind} ${name}. Its replicas restart.`,
  "confirm.basicAuth": (route: string) =>
    `Ask for a password on every domain of Ingress ${route}. The cluster keeps only a bcrypt hash of it.`,
  "confirm.basicAuthOff": (route: string) => `Stop asking for a password on Ingress ${route}.`,
  "confirm.errorPageHtml": "Write the project's error pages to the page server's config file.",
  "confirm.pageServer":
    "Deploy the Error pages template to the project as its page server, with these pages.",
  "confirm.template": (template: string) => `Deploy the ${template} template in the project.`,
  "confirm.errorPages": (route: string) =>
    `Show the project's error pages on every domain of Ingress ${route}.`,
  "confirm.errorPagesOff": (route: string) => `Stop showing the error pages on Ingress ${route}.`,
  "confirm.ipAllow": (route: string, list: string) =>
    `Only ${list} can open the domains of Ingress ${route}. Any other address gets 403 Forbidden.`,
  "confirm.ipAllowOff": (route: string) => `Anyone can open the domains of Ingress ${route} again.`,
  "confirm.egressLimit": (name: string, list: string) =>
    `${name} can connect only to ${list}, this project and DNS.`,
  "confirm.egressLimitNone": (name: string) => `${name} can connect only to this project and DNS.`,
  "confirm.egressLimitOff": (name: string) => `${name} can connect anywhere again.`,
  "confirm.maintenance": (host: string) =>
    `Show the maintenance page on ${host}. The service keeps running.`,
  "confirm.maintenanceOff": (host: string) => `Send visitors of ${host} to the service again.`,
  "confirm.copyS3Keys": (name: string, destination: string) =>
    `Copy the keys of ${destination} into secret ${name}.`,
  "confirm.deployKey": (name: string) => `Create secret ${name} with the new deploy key.`,
  "confirm.isolates": (name: string) =>
    `${name} then accepts and sends only what this and other policies allow; the cluster blocks the rest.`,
  "confirm.delete": (name: string) => `Delete ${name}. This can't be undone.`,
  "confirm.deleteDatabase": (name: string) =>
    `Delete ${name} and its data for good. Backups in object storage are kept.`,
  "action.restart": "Restart",
  "action.deploy": "Deploy",
  "action.rebuild": "Build again",
  "action.upload": "Upload",
  "action.saveBuildOptions": "Save build options",
  "action.autoUpdateOn": "Turn on",
  "action.autoUpdateOff": "Turn off",
  "action.saveVariables": "Save variables",
  "action.saveValues": "Save values",
  "action.turnOn": "Turn on",
  "action.turnOff": "Turn off",
  "action.makeStrict": "Make Strict",
  "action.makeCompatible": "Make Compatible",
  "action.addDomain": "Add domain",
  "action.renameDomain": "Rename domain",
  "action.removeDomain": "Remove domain",
  "action.rollback": (revision: number) => `Roll back to revision ${revision}`,
  "action.grow": (size: string) => `Grow to ${size}`,
  "action.addVolume": "Add volume",
  "action.saveMount": "Save mount",
  "action.removeVariable": "Remove variable",
  "action.removeVolume": "Remove volume",
  "action.swapClaim": "Switch volume",
  "action.deleteVolume": "Delete volume",
  "action.saveVolumes": "Save volumes",
  "action.saveHealthChecks": "Save health checks",
  "action.scale": (replicas: number) => `Scale to ${replicas}`,
  "action.scaleMany": (n: number) => `Scale ${n} services`,
  "action.restartMany": (n: number) => `Restart ${n} services`,
  "action.pause": "Pause",
  "action.resume": "Resume",
  "action.saveSchedule": "Save schedule",
  "action.pauseMany": (n: number) => `Pause ${n} services`,
  "action.saveResources": "Save resources",
  "action.save": "Save",
  "action.applyMany": (n: number) => `Apply ${n} changes`,
  "action.saveYaml": "Save YAML",
  "action.isolate": "Isolate",
  "action.allow": "Allow",
  "action.create": "Create",
  "action.redirect": "Redirect",
  "action.stopRedirect": "Remove redirect",
  "action.askPassword": "Ask for password",
  "action.openPort": "Open port",
  "action.closePort": "Close port",
  "action.stopPassword": "Remove password",
  "action.savePages": "Save pages",
  "action.startMaintenance": "Start maintenance",
  "action.endMaintenance": "End maintenance",
  "action.showErrorPages": "Show error pages",
  "action.hideErrorPages": "Remove error pages",
  "action.limitAddresses": "Limit addresses",
  "action.removeLimit": "Remove limit",
  "action.delete": "Delete",
  // Toasts
  "toast.applied": "Applied",
  "toast.view": "View",
  "toast.undo": "Undo",
  "toast.undone": "Undone",
  "undo.scaleUp": (n: number) => `Scale ${n} services back up`,
  "undo.scaleBack": (n: number) => `Scale ${n} services back`,
  "undo.many": (n: number) => `Undo ${n} changes`,
  "toast.managed": (tool: string) => `Managed by ${tool}`,
  "toast.managedHelm": "Change it in the chart's values and upgrade the release.",
  "toast.managedGit": (tool: string) => `Change it in the Git repository ${tool} syncs from.`,
  "title.env": (service: string) => `Variables for ${service}`,
  "title.data": (kind: string, name: string) => `${kind} ${name}`,
  "title.restart": (service: string) => `Restart ${service}`,
  "title.configReload": (service: string) => `Auto-restart ${service}`,
  "title.deployBuild": (service: string) => `Deploy the newest build to ${service}`,
  "title.rebuild": (build: string) => `Build ${build} again`,
  "title.buildOptions": (build: string) => `Change how ${build} is built`,
  "title.autoUpdateOn": (service: string) => `Auto-deploy ${service}`,
  "title.autoUpdateOff": (service: string) => `Stop auto-deploying ${service}`,
  "title.addDomain": (host: string) => `Add ${host}`,
  "title.renameDomain": (from: string, to: string) => `${from} → ${to}`,
  "title.removeDomain": (host: string) => `Remove ${host}`,
  "yaml.edit": "Edit",
  "yaml.editSecret": "Values left as (hidden) are kept.",
  "yaml.notEditable": "Kubernetes manages this kind itself; edit the object that owns it.",
  "title.editYaml": (name: string) => `Edit ${name}`,
  "title.delete": (kind: string, name: string) => `Delete ${kind} ${name}`,
  "yaml.delete": "Delete",
  "yaml.notDeletable": "Kubernetes creates this kind itself; delete the object that owns it.",
  "title.rollback": (service: string, revision: number) => `Roll back ${service} to #${revision}`,
  "releases.none": "Each time the image or a setting changes, a new deployment shows up here.",
  "releases.noneTask": "Jobs and CronJobs keep no releases.",
  "releases.current": "Running",
  "releases.compare": "Compare",
  "releases.rollback": "Roll back",
  "releases.diff": (from: number, to: number) => `#${from} → #${to}`,
  "releases.same": "The two templates are the same.",
  "form.compute": "Compute",
  "form.health": "Health checks",
  "form.jobs": "Jobs run once. Change their template in YAML.",
  "form.replicas": "Replicas",
  "form.cpuRequest": "Reserved CPU",
  "form.notSet": "Not set",
  "form.cpuUnset": "Not set: nothing reserved. For example 0.25 cores.",
  "form.memoryUnset": "Not set: nothing reserved. For example 256 MiB.",
  "form.noLimit": "Not set: no limit",
  "form.memoryRequest": "Reserved memory",
  "form.cpuLimit": "Maximum CPU",
  "form.memoryLimit": "Maximum memory",
  "form.computeHint":
    "The node sets the reserved amount aside for each replica. When a replica uses more memory than its maximum, its container restarts; CPU above the maximum is slowed down.",
  "form.largestNode": (cores: string, memory: string) => `Largest node: ${cores} cores, ${memory}`,
  "form.badQuantity": "Enter an amount such as 0.25 cores or 512 MiB.",
  "form.badReplicas": "Enter a whole number from 0 to 1000.",
  "form.badNumber": "Enter a whole number above 0.",
  "form.probe.readiness": "Traffic check",
  "form.probe.liveness": "Restart check",
  "form.probe.startup": "Startup check",
  "form.probeEffect.readiness": "Traffic goes only to replicas that pass.",
  "form.probeEffect.liveness": "The cluster restarts a container that keeps failing.",
  "form.probeEffect.startup": "Holds the other checks until the app has started.",
  "form.probeCheck": "Check",
  "form.probeNone": "None",
  "form.probeExec": "Command",
  "form.probePath": "Path",
  "form.probePort": "Port",
  "form.probeCommand": "Command",
  "form.probeCommandMissing": "Enter a command.",
  "form.probePeriod": "Every (s)",
  "form.probeFailures": "Failures",
  "form.probePreset": (port: string) => `HTTP on ${port}`,
  "form.computeBlocked": "Fix compute to save.",
  "form.probeBlocked": (probe: string) => `Fix the ${probe.toLowerCase()} to save.`,
  "form.size": "Size",
  "form.sizeHint": "A volume can grow but never shrink.",
  "form.readOnly": "Read-only",
  "form.addVolume": "Add a volume",
  "form.volumeName": "Name",
  "form.statefulVolumes": "A StatefulSet's volumes are set when it is created.",
  "form.advanced": "Advanced",
  "form.claimName": "Disk name",
  "form.claimNameHint": "Creates a new disk. The name must not be in use already.",
  "form.accessMode": "Access mode",
  "form.accessModeHint": "Many-node modes need storage that supports them.",
  "ops.managedEdit": (tool: string) => `Managed by ${tool}: change it at its source`,

  // Editing
  "edit.add": "Add",
  "edit.review": "Review changes",
  "edit.cancel": "Cancel",
  "edit.edit": (name: string) => `Edit ${name}`,
  "edit.remove": (name: string) => `Remove ${name}`,
  "edit.removeWithRoute": (host: string, route: string) =>
    `Remove ${host} and route ${route}, which has no other domain`,
  "edit.undo": (name: string) => `Keep ${name}`,
  "vars.edit": "Edit",
  "vars.addRow": "Add a variable",
  "vars.pasteHint": "Paste NAME=value lines into a name to add them all.",
  "vars.dotenv": "Variables as .env",
  "vars.dotenvHint":
    "One NAME=value per line. Quotes, export and # comments work; a line left out is removed.",
  "vars.unread": (lines: number[]) =>
    `${lines.length === 1 ? "Line" : "Lines"} ${new Intl.ListFormat("en").format(lines.map(String))} ${lines.length === 1 ? "is" : "are"} not NAME=value.`,
  "vars.blocked": (n: number) => `Fix ${plural(n, "name", "names")} to save.`,
  "vars.edited": (n: number) => plural(n, "change", "changes"),
  "vars.state.added": "New",
  "vars.state.changed": "Changed",
  "vars.state.removed": "Removed",
  "vars.undo": "Undo",
  "vars.namePlaceholder": "NAME",
  "vars.valueOf": (name: string) => `Value of ${name}`,
  "vars.problem.empty": "Enter a name.",
  "vars.problem.invalid": "Use letters, digits, _, - and ., not starting with a digit.",
  "vars.problem.duplicate": "Another variable has this name.",
  "vars.problem.taken": "A config file, secret or Kubernetes already sets this name.",
  "vars.editThere": "Change it where it is kept",
  "vars.imported": "Every key of a config file or secret",
  "vars.inSecret": (secret: string, key: string) => `${key} in secret ${secret}`,
  "vars.fromDatabase": (database: string) => `From database ${database}`,
  "vars.allOfSecret": (secret: string) => `Every key in secret ${secret}`,
  "vars.inConfig": (file: string, key: string) => `${key} in config file ${file}`,
  "vars.allOfConfig": (file: string) => `Every key in config file ${file}`,
  "vars.fromFile": (path: string) => `The file ${path}`,
  "vars.field.podName": "The replica's name",
  "vars.field.namespace": "The project's Namespace",
  "vars.field.podIP": "The replica's IP address",
  "vars.field.hostIP": "The node's IP address",
  "vars.field.nodeName": "The node's name",
  "vars.field.serviceAccount": "The replica's service account",
  "vars.field.cpuLimit": "The CPU maximum",
  "vars.field.memoryLimit": "The memory maximum",
  "vars.field.cpuRequest": "The reserved CPU",
  "vars.field.memoryRequest": "The reserved memory",
  "vars.field.other": (path: string) => `The replica's ${path}`,
  "kube.details": "Details for Kubernetes users",
  "describe.view": "View",
  "describe.describe": "Description",
  "kind.Pod": "Replica",
  "kind.PersistentVolumeClaim": "Disk",
  "kind.Secret": "Secret",
  "kind.ConfigMap": "Config file",
  "vars.add": "Add variable",
  "vars.name": "Name",
  "vars.value": "Value",
  "vars.container": "Container",
  "data.addKey": "Add key",
  "data.key": "Key",
  "data.readers": "Read by",
  "reload.switch": "Restart when its config files or secrets change",
  "reload.hint":
    "Changing this service's variables, image or resources redeploys it right away. A change to a config file or secret it uses only takes effect after a restart; turn this on to restart automatically.",
  "reload.on":
    "On: a change to a config file or secret it uses restarts it, whoever makes the change.",
  "restart.rule":
    "Changes to variables, the image or resources redeploy right away. A change to a config file or secret takes effect when the service restarts, unless it restarts on such changes automatically.",
  "reload.notInstalled":
    "Reloader isn't installed in this cluster, so nothing restarts yet. Ask a cluster administrator to install Stakater Reloader.",
  "reload.stopped":
    "Reloader is installed but not running, so nothing restarts until it runs again.",
  "reload.how": "How it works",
  "reload.mechanism":
    "Stakater Reloader, running in the cluster, restarts it. This setting is the annotation reloader.stakater.com/auto on the workload; it restarts nothing by itself.",
  "reload.named": (names: string) =>
    `Its annotations also name ${names}, which restart it whether this is on or not.`,
  "reload.unknown": "With your rights, Zeabur can't see whether Reloader runs in this cluster.",
  "reload.running": "Reloader runs in this cluster.",
  "data.reveal": "Reveal values",
  "data.binary": "Binary",
  "data.noChanges": "No changes.",
  "net.addDomain": "Add domain",
  "net.domain": "Domain",
  "net.routeKind": "Route",
  "net.viaIngress": "Ingress (default)",
  "net.viaGateway": (gateway: string, listener: string) =>
    `Gateway ${gateway}, listener ${listener}`,
  "net.gatewayHttps": "HTTPS comes from the Gateway's listener.",
  "diagnosis.title": "What's wrong",
  "diagnosis.evidence": "Evidence",
  "diagnosis.replica": "Replica",
  "diagnosis.exitCode": "Exit code",
  "diagnosis.action.logs": "Open logs",
  "diagnosis.action.memory": "Change memory",
  "diagnosis.action.healthChecks": "Health checks",
  "diagnosis.action.networking": "Networking",
  "diagnosis.action.events": "Open events",
  "diagnosis.action.replicas": "Change replicas",
  "diagnosis.action.reservation": "Change reservation",
  "diagnosis.action.variables": "Variables",
  "diagnosis.action.storage": "Storage",
  "diagnosis.action.yaml": "Edit YAML",
  "diagnosis.action.security": "Security",
  "gap.title": "Missing setup",
  "gap.warning": "Warning",
  "gap.advice": "Advice",
  "gap.unchecked": (count: number) =>
    `${count} ${count === 1 ? "service has" : "services have"} no traffic check`,
  "gap.uncheckedWhy":
    "Without one, Running only means the container started. Add one under each service's Settings.",
  "gap.uncheckedNames": "Show services",
  "gap.fix": "Fix",
  "gap.install": "Install",
  "gap.setUpBackups": "Set up backups",
  "gap.openSettings": "Open settings",
  "fix.resources": (container: string) => `Set CPU and memory for ${container}`,
  "fix.readiness": (container: string) => `Add a traffic check to ${container}`,
  "fix.addVolume": (container: string, path: string) => `Keep ${path} of ${container} on a volume`,
  "fix.isolateProject": (project: string) => `Isolate the services of ${project}`,
  "gap.unreadable": "Some checks need rights you don't have.",
  "install.title": "Install",
  "install.helm": "Run these commands where helm and kubectl can reach the cluster.",
  "install.noController":
    "Zeabur can install it for you when the cluster runs K3s, RKE2, Flux or Argo CD.",
  "install.installed": "Already installed in this cluster.",
  "install.yourself": "Apply it with kubectl, or commit it to your GitOps repository.",
  "install.blocked.k3sPackaged":
    "Zeabur doesn't install this one itself: K3s and RKE2 ship their own copy, which has to be turned off on the servers first.",
  "install.blocked.inputs": "Zeabur doesn't install this one itself: it needs values you fill in.",
  "install.blocked.manifests":
    "Zeabur doesn't install this one itself: part of it is plain manifests, not a chart.",
  "install.blocked.privileged":
    "Zeabur doesn't install this one itself: it needs access to the host, so its Namespace must be set to Unrestricted first. The text below does that.",
  "install.tool.k3s": "the K3s/RKE2 Helm controller",
  "install.tool.flux": "Flux",
  "install.tool.argoCd": "Argo CD",
  "install.planWhat": (namespace: string, tool: string) =>
    `Zeabur creates these objects in ${namespace}, and ${tool} installs the chart from them. You check each object before anything is created.`,
  "install.planAction": "Review and install",
  "install.planTitle": (name: string) => `Install ${name}`,
  "install.after": (tool: string) =>
    `${tool} installs the chart once the objects exist, usually within a few minutes.`,
  "install.afterRemove": (tool: string) =>
    `${tool} installs the chart once the objects exist, usually within a few minutes. To remove it, delete these objects.`,
  "install.profile": "Profile",
  "install.singleNode": "Single node",
  "install.multiNode": "Several nodes",
  "install.forCluster": "fits this cluster",
  "install.notes": "Notes",
  "install.needs": "Install first",
  "install.security": "Permissions",
  "install.clusterWide": "Cluster-wide RBAC",
  "install.privileged": "Privileged",
  "install.connectsTo": "Connects to",
  "install.credentials": "Credentials",
  "install.risks": "Risks",
  "install.inputs": "Values to fill in",
  "install.generated": "generated",
  "install.required": "required",
  "install.alsoAdds": "Also adds values to",
  "install.k3sPackaged": (addon: string) =>
    `On K3s, first turn off the packaged ${addon}: add it to disable in /etc/rancher/k3s/config.yaml on every server, then restart k3s.`,
  "install.rke2Packaged": (name: string) =>
    `On RKE2, add ${name} to disable in /etc/rancher/rke2/config.yaml on every server, then restart rke2-server.`,
  "install.rke2PackagedIngress":
    "On RKE2, set ingress-controller: none in /etc/rancher/rke2/config.yaml on every server, then restart rke2-server.",
  "install.k3sPackagedTraefik":
    "That deletes the Traefik CRDs and any IngressRoute or Middleware objects; Ingress traffic stops until this one runs.",
  "install.k3sPackagedMetrics": "The metrics API is unavailable until this one runs.",
  "install.k3sPackagedOther": "Its workloads stop until this one runs.",
  "reach.title": "Connection rules",
  "reach.addPolicy": "Add rule",
  "reach.isolate": "Isolate this service",
  "reach.allowIngress": "Allow traffic from public domains",
  "reach.planIsolate": (name: string) => `Isolate ${name}`,
  "reach.planAllowIngress": (name: string) => `Let public domains reach ${name}`,
  "reach.planAllowFrom": (from: string, to: string) => `Let ${from} reach ${to}`,
  "reach.policies": "Rules",
  "reach.noPolicies": "This project has no connection rules, so every connection is allowed.",
  "reach.noneSelect": "No rule applies to this service, so every connection is allowed.",
  "reach.in": "Incoming",
  "reach.out": "Outgoing",
  "reach.otherProjects": "Other projects",
  "reach.dns": "DNS lookups",
  "reach.outside": "Outside the cluster",
  "reach.cni": "Whether these rules are enforced depends on the cluster's network plugin (CNI).",
  "reach.allowed": "Allowed",
  "reach.limited": "Partly",
  "reach.blocked": "Blocked",
  "reach.edgeBlocked": "Blocked by a connection rule",
  "reach.edgeLimited": "Only partly allowed by connection rules",
  "reach.allowFromPick": "Allow from a service…",
  "reach.rulesApply": (n: number) => plural(n, "rule applies", "rules apply"),
  "reach.rules": (n: number) => plural(n, "rule", "rules"),
  "reach.lead": (clauses: string[], capital: boolean) => {
    const text = clauses.join("; ")
    return `${capital ? text.charAt(0).toUpperCase() + text.slice(1) : text}.`
  },
  "reach.lead.none": (name: string) => `${name} accepts no other service`,
  "reach.lead.all": (name: string) => `${name} accepts every service in the project`,
  "reach.lead.only": (name: string, n: number) =>
    `${name} accepts only ${plural(n, "service", "services")}`,
  "reach.services": (n: number) => plural(n, "service", "services"),
  "reach.lead.otherProjects": "other projects",
  "reach.lead.domains": "public domains",
  "reach.lead.blocked": (list: string, one: boolean) => `${list} ${one ? "is" : "are"} blocked`,
  "reach.lead.limited": (list: string, one: boolean) =>
    `${list} ${one ? "is" : "are"} partly allowed`,
  "reach.lead.allowed": (list: string, one: boolean) => `${list} ${one ? "is" : "are"} allowed`,
  "reach.more": (n: number) => `+${n} more`,
  "reach.filter": "Filter services",
  "reach.service": "Service",
  "reach.via": "Rule",
  "reach.allow": "Allow",
  "reach.noMatch": "No service matches.",
  "alerts.title": "Runtime alerts",
  "alerts.nodes": "Node alerts",
  "alerts.severity.critical": "Critical",
  "alerts.severity.high": "High",
  "alerts.severity.medium": "Medium",
  "alerts.severity.low": "Low",
  "alerts.severity.info": "Info",
  "alerts.output": "Falco's message",
  "alerts.recent": "Up to 1,000 recent alerts; when full, Falco drops the least serious first.",
  "alerts.seeing": "Seeing runtime alerts",
  "alerts.filter": "Filter alerts",
  "backups.title": "Backups",
  "backups.namespace": "All objects in the namespace",
  "backups.unscheduled": "No schedule",
  "backups.none": "Not backed up",
  "backups.noRun": "No backup yet",
  "backups.noProvider": "To back up volumes, install a backup component.",
  "backups.seeComponents": "Open components",
  "backups.refused": (module: string) => `You don't have permission to read ${module}'s backups.`,
  "backups.details": "Schedules and runs",
  "backups.paused": "Paused",
  "backups.succeeded": "Succeeded",
  "backups.partiallyFailed": "Partly failed",
  "backups.failed": "Failed",
  "backups.running": "Running",
  "backups.unknown": "Unknown",
  "backups.veleroVolumesNeedSnapshots": "volume data only with snapshots",
  "backups.scheduledOf": (scheduled: number, total: number) => `${scheduled}/${total} scheduled`,
  "backups.volume.set": "Back up this volume",
  "backups.volume.body":
    "K8up copies this volume's files to S3-compatible storage on a schedule. The keys and the repository password stay in a secret of this project, and the backups keep running without Zeabur.",
  "backups.volume.folderHint": (folder: string) => `${folder} when left empty.`,
  "backups.volume.keepLast": "Keep the newest",
  "backups.volume.keepDaily": "Keep daily, in days",
  "backups.volume.keepWeekly": "Keep weekly, in weeks",
  "backups.volume.problem.keep": "Whole numbers: 1 to 365 days, up to 100 newest, up to 520 weeks.",
  "backups.volume.password": (secret: string) =>
    `Zeabur makes a repository password and keeps it in the secret ${secret}. Without it the backups can't be read, so keep a copy somewhere else.`,
  "backups.volume.root":
    "The service runs as root, so its backups do too and can read every file of the volume.",
  "backups.volume.user": (user: number) =>
    `Backups run as user ${user}, as the service does, so they meet Strict security.`,
  "backups.volume.imageUser":
    "The service sets no user, so backups run as K8up's non-root user and meet Strict security. They skip files that only the service's user may read: set a user in the service's security settings to include them.",
  "backups.volume.restricted":
    "This project allows only Strict services and refuses replicas that run as root, but backups of this service run as root. Set the service to Strict, or ask an administrator to let the project allow Compatible services.",
  "backups.volume.openSecurity": "Open security settings",
  "backups.volume.stop": "Stop backups",
  "backups.volume.planStop": (claim: string) => `Stop backups of ${claim}`,
  "backups.volume.stopSaid": (claim: string, secret: string) =>
    `K8up stops backing up ${claim}. The backups taken stay in the bucket, and secret ${secret} keeps the password that restores them.`,
  "backups.volume.long": "K8up can't select this volume: its name is longer than 63 characters.",
  "backups.volume.notInstalled":
    "Volume backups need K8up, which this cluster doesn't have. An administrator can install it.",
  "backups.volume.openK8up": "Open K8up",
  "backups.volume.planNow": (claim: string) => `Back up ${claim} now`,
  "backups.volume.keeps": (list: string) => `keeps ${list}`,
  "backups.volume.keep.last": (n: number) => `the newest ${n}`,
  "backups.volume.keep.hourly": (n: number) => `${n} hourly`,
  "backups.volume.keep.daily": (n: number) => `${n} daily`,
  "backups.volume.keep.weekly": (n: number) => `${n} weekly`,
  "backups.volume.keep.monthly": (n: number) => `${n} monthly`,
  "backups.volume.keep.yearly": (n: number) => `${n} yearly`,
  "backups.volume.runs": "Backups of these volumes",

  "backups.volume.liveFiles":
    "Files a running database is writing may be copied in an unfinished state. Back a database up with its own backups, or have K8up back up its dump with",
  "volumeRestore.points": (claim: string) => `${claim} can go back to how it was at a backup.`,
  "volumeRestore.noPoints": (claim: string) =>
    `${claim} has no backup point to restore yet. One shows here after the first backup.`,
  "volumeRestore.new": "New volume from a backup point",
  "volumeRestore.inPlace": "Stop the service, clear the volume, restore in place",
  "volumeRestore.toItself": (claim: string) => `Into ${claim} itself`,
  "volumeRestore.toVolume": (into: string) => `Into new volume ${into}`,
  "volumeRestore.check":
    "K8up can report a restore as done even when it failed, so check the files before you use them.",
  "volumeRestore.swap": "Switch the service to the new volume",
  "volumeRestore.swapTitle": (service: string, into: string) => `Switch ${service} to ${into}`,
  "volumeRestore.swapDowntime": (service: string) =>
    `${service} stops, then starts again, so it is down for a short time.`,
  "volumeRestore.keep": (claim: string) =>
    `The original volume ${claim} is kept. Delete it when you no longer need it.`,
  "volumeRestore.start": "Start the service",
  "volumeRestore.startTitle": (service: string) => `Start ${service}`,
  "volumeRestore.inPlaceTitle": (claim: string) => `Restore ${claim} in place`,
  "volumeRestore.newTitle": (claim: string) => `New volume from a backup point of ${claim}`,
  "volumeRestore.newNote": (into: string, when: string) =>
    `The new volume ${into} holds the files as they were at the backup of ${when}.`,
  "volumeRestore.asUser": "The restore runs as the same user as the backups.",
  "volumeRestore.cleared": (claim: string, when: string) =>
    `${claim} gets the files of the backup of ${when}. Files added or changed after it are deleted.`,
  "volumeRestore.clearedAll": (claim: string) =>
    `Every file now in ${claim} is replaced by the backup point. Files added or changed after it are deleted.`,
  "volumeRestore.stops": (service: string) =>
    `${service} stops. You start it again once the restore is done.`,
  "volumeRestore.noUndo": "This can't be undone.",
  "volumeRestore.warnOverwrite":
    "Restoring in place overwrites the volume. Files added after the backup are removed.",
  "volumeRestore.warnRepo":
    "Anyone who can restore can read every backup in this project's backup repository.",
  "volumeRestore.understood": (claim: string) =>
    `I understand the files now in ${claim} are cleared`,
  "volumeRestore.onlyBackups": "Only the times of backups can be chosen, not any time.",
  "volumeRestore.point": "Backup point",
  "volumeRestore.zone": "In your time zone.",
  "volumeRestore.name": "New volume name",
  "volumeRestore.sameAs": (size: string, storageClass: string) =>
    `Same size and storage class as the original: ${size}, ${storageClass}.`,
  "volumeRestore.badName": "Lowercase letters, digits and hyphens, and not the original's name.",
  // Cluster → Backups (Velero)
  "cluster.backups": "Backups",
  "clusterBackups.what": "the cluster's backups",
  "clusterBackups.verdict.noLocation": "Backups aren't set up. Choose where they go first.",
  "clusterBackups.verdict.locationUnavailable":
    "Velero can't reach the backup location, so backups fail.",
  "clusterBackups.verdict.noSchedule": "Backups have somewhere to go, but nothing runs them yet.",
  "clusterBackups.verdict.paused": "Backups are paused.",
  "clusterBackups.verdict.waiting": "Backups are set up. None has run yet.",
  "clusterBackups.verdict.failed": "The last backup didn't finish.",
  "clusterBackups.verdict.running": "A backup is running.",
  "clusterBackups.verdict.ok": "Backups run on schedule.",
  "clusterBackups.lastGood": "Last complete backup",
  "clusterBackups.databasesNote":
    "Databases (PostgreSQL, MongoDB) are backed up on their own, from each database's Backups tab. These backups cover the other volumes and every Kubernetes object.",
  "clusterBackups.notInstalled": "Backups need Velero",
  "clusterBackups.notInstalledBody":
    "Velero copies the cluster's objects and the files in its volumes to S3-compatible storage on a schedule. Install it from Components, then come back here to set it up.",
  "clusterBackups.install": "Install Velero",
  "clusterBackups.location": "Where backups go",
  "clusterBackups.setUp": "Set up",
  "clusterBackups.edit": "Edit",
  "clusterBackups.cancel": "Cancel",
  "clusterBackups.bucket": "Bucket",
  "clusterBackups.endpoint": "Endpoint",
  "clusterBackups.endpointHint": "Leave empty for AWS S3. For R2, B2 or MinIO, their S3 address.",
  "clusterBackups.awsS3": "AWS S3",
  "clusterBackups.folder": "Folder",
  "clusterBackups.folderHint": "Optional. Keeps these backups apart from other files.",
  "clusterBackups.region": "Region",
  "clusterBackups.regionHint": "us-east-1 when left empty.",
  "clusterBackups.accessKey": "Access key ID",
  "clusterBackups.secretKey": "Secret access key",
  "clusterBackups.keepKeys": "Leave both empty to keep the keys it has.",
  "clusterBackups.status": "Status",
  "clusterBackups.phase.available": "Reachable",
  "clusterBackups.phase.unavailable": "Can't reach",
  "clusterBackups.phase.unknown": "Not checked yet",
  "clusterBackups.checked": "checked",
  "clusterBackups.outside":
    "Use storage outside this cluster, such as Cloudflare R2, Backblaze B2 or AWS S3, so backups outlive the cluster.",
  "clusterBackups.inside":
    "This endpoint looks like storage inside this cluster. If the cluster is lost, its backups go with it.",
  "clusterBackups.problem.bucket": "3 to 63 lowercase letters, digits, dots and hyphens.",
  "clusterBackups.problem.endpoint": "An http:// or https:// address.",
  "clusterBackups.problem.prefix": 'No spaces or "..".',
  "clusterBackups.problem.keys": "Both keys are needed.",
  "clusterBackups.problem.bothKeys": "Enter both keys, or neither to keep the ones it has.",
  "clusterBackups.problem.keep": "From 1 to 365 days.",
  "clusterBackups.problem.projects": "Choose at least one project.",
  "clusterBackups.schedule": "Schedule",
  "clusterBackups.runs": "Runs",
  "clusterBackups.keeps": "Keeps",
  "clusterBackups.keepDays": (days: number) => plural(days, "day", "days"),
  "clusterBackups.covers": "Covers",
  "clusterBackups.wholeCluster": "Whole cluster",
  "clusterBackups.wholeClusterHint": "Every project and namespace, including ones added later.",
  "clusterBackups.chosenProjects": "Chosen projects",
  "clusterBackups.chosenProjectsHint": "Only the projects you select.",
  "clusterBackups.volumes": "Volumes",
  "clusterBackups.fsOn": "Files in volumes are copied",
  "clusterBackups.fsOff": "Objects only; files in volumes aren't copied",
  "clusterBackups.time": "Time",
  "clusterBackups.daily": (hour: string) => `Daily at ${hour} (UTC)`,
  "clusterBackups.asSet": (words: string) => `As set: ${words}`,
  "clusterBackups.keep": "Keep, in days",
  "clusterBackups.pause": "Pause",
  "clusterBackups.resume": "Resume",
  "clusterBackups.noSchedule": "No schedule yet. A daily backup kept for 7 days is a good start.",
  "clusterBackups.needLocation": "Set where backups go first.",
  "clusterBackups.other": (name: string) => `${name}, set up outside Zeabur`,
  "clusterBackups.backups": "Backups",
  "clusterBackups.none": "No backups yet.",
  "clusterBackups.newest": (shown: number, total: number) => `The newest ${shown} of ${total}.`,
  "clusterBackups.now": "Back up now",
  "clusterBackups.items": (done: number, total?: number | null) =>
    total && total !== done
      ? `${done.toLocaleString("en")} of ${total.toLocaleString("en")} items`
      : `${done.toLocaleString("en")} ${done === 1 ? "item" : "items"}`,
  "clusterBackups.keptUntil": (date: string) => `kept until ${date}`,
  "clusterBackups.errors": (n: number) => plural(n, "error", "errors"),
  "clusterBackups.warnings": (n: number) => plural(n, "warning", "warnings"),
  "clusterBackups.failure.locationUnavailable":
    "Velero couldn't reach the backup location. Check the bucket, endpoint and keys.",
  "clusterBackups.failure.invalid": "Velero refused the backup's settings.",
  "clusterBackups.failure.itemsFailed":
    "Some objects or volume files weren't copied; the rest is saved. Velero's logs for this backup name each one.",
  "clusterBackups.failure.interrupted":
    "Velero restarted while this backup ran. The next one runs on schedule.",
  "clusterBackups.failure.other": "The backup failed.",
  "clusterBackups.restoreAction": "Restore",
  "clusterBackups.problem.restoreNone": "Choose at least one namespace.",
  "clusterBackups.restores": "Restores",
  "clusterBackups.restoredFrom": (backup: string) => `From ${backup}`,
  "clusterBackups.copyAction": "New project from backup",
  "clusterBackups.restoreGoneAction": "Restore deleted project",
  "clusterBackups.chooseAction": "Restore a project",
  "clusterBackups.deletedChoice": (name: string) => `${name} (deleted)`,
  "clusterBackups.readingDeleted": "Looking for deleted projects in the backup…",
  "clusterBackups.deletedUnread": (reason: string) =>
    `Zeabur couldn't read which deleted projects the backup holds: ${reason}`,
  "clusterBackups.download": "Download backup",
  "clusterBackups.restoreFrom": "Project to restore",
  "clusterBackups.chooseProject": "Choose a project",
  "clusterBackups.copyName": "New project name",
  "clusterBackups.copyHint": (source: string) =>
    `${source} keeps running; the backup goes into this new project.`,
  "clusterBackups.problem.intoTaken": "Choose a name no project uses.",
  "clusterBackups.restoredInto": (source: string, into: string) => `${source} → ${into}`,
  "clusterBackups.openCopy": "Open project",
  "clusterBackups.plan.copy": (source: string) => `Restore ${source} from backup`,
  "clusterBackups.copy.into": (source: string, into: string) =>
    `The backup goes into a new project, ${into}, with its own copy of every service, volume and database. ${source} is not touched.`,
  "clusterBackups.copy.keep": (source: string) =>
    `The original project ${source} is kept. Delete it when you no longer need it.`,
  "clusterBackups.copy.amount": (cpu: string, memory: string) => `${cpu} and ${memory}`,
  "clusterBackups.copy.short": (need: string, free: string) =>
    `Both projects run at once. The new one asks for ${need}, but the nodes have ${free} left, so some services may not start. Stop services you don't need, or add a node.`,
  "clusterBackups.copy.fits": (need: string, free: string) =>
    `Both projects run at once: the new one asks for ${need}, and the nodes have ${free} left.`,
  "clusterBackups.copy.roomUnknown": (need: string) =>
    `Both projects run at once, and the new one asks for ${need}. Zeabur couldn't read how much the nodes have left.`,
  "clusterBackups.copy.webhooks": (source: string) =>
    `Auto deploy and Git webhooks still go to ${source}.`,
  "clusterBackups.goneBody": (source: string) =>
    `${source} no longer exists, so it comes back under its own name. Its domains come back in a last step, once you've checked it.`,
  "clusterBackups.copy.sameName": (source: string) =>
    `${source} no longer exists, so the backup goes back under its own name.`,
  "clusterBackups.copy.domainsLast": (routes: string) =>
    `Domains stay out of the restore (${routes}). Once you've checked the project, bring them back with "Bring domains back".`,
  "clusterBackups.copy.domainsLastAny":
    'Domains stay out of the restore. Once you\'ve checked the project, bring them back with "Bring domains back".',
  "clusterBackups.copy.cronJobs": (n: number) =>
    `${plural(n, "scheduled job starts", "scheduled jobs start")} paused, so nothing runs twice. Resume each on its page when the new project is ready:`,
  "clusterBackups.copy.services": (source: string, into: string, example: string) =>
    `Values that use these services' addresses, such as ${example}.${source}.svc, still reach ${source}. Once the restore is done, "Rewrite addresses" points them at ${into}:`,
  "clusterBackups.copy.secrets": (source: string) =>
    `These secrets may also hold addresses in ${source}. "Rewrite addresses" reads and changes them too:`,
  "clusterBackups.copy.unread": (reason: string) =>
    `Zeabur couldn't read what the backup holds, so it can't list it here: ${reason}`,
  "clusterBackups.move.action": "Move domains",
  "clusterBackups.attach.action": "Bring domains back",
  "clusterBackups.attach.title": (to: string) => `Bring domains back to ${to}`,
  "clusterBackups.attach.hosts": (hosts: string, to: string) =>
    `${hosts} come back to ${to} from the backup.`,
  "clusterBackups.attach.brief": "Certificates for these domains are requested again.",
  "clusterBackups.attach.held": (hosts: string, project: string, to: string) =>
    `${project} serves ${hosts} right now. Move them from ${project} to ${to} instead.`,
  "clusterBackups.attach.move": (project: string) => `Move domains from ${project}`,
  "clusterBackups.domainsBack": (to: string) => `Domains back to ${to}`,
  "clusterBackups.thing.ConfigMap": (name: string) => `the rule ${name} that pauses scheduled jobs`,
  "clusterBackups.move.title": (into: string) => `Move domains to ${into}`,
  "clusterBackups.move.hosts": (hosts: string, from: string, to: string) =>
    `${hosts} move from ${from} to ${to}.`,
  "clusterBackups.move.managed": (routes: string) =>
    `A tool manages these routes, so they stay: ${routes}.`,
  "clusterBackups.move.brief":
    "Each domain is unreachable for a moment while it moves, and the new project gets its certificate again.",
  "clusterBackups.rewrite.action": "Rewrite addresses",
  "clusterBackups.rewrite.title": (into: string) => `Rewrite addresses in ${into}`,
  "clusterBackups.rewrite.note": (source: string, into: string) =>
    `Values that name ${source}.svc will name ${into}.svc.`,
  "clusterBackups.rewrite.none": (source: string) =>
    `No variable, config file or secret in the new project names ${source}.`,
  "clusterBackups.thing.Restore": (name: string) => `restore ${name}, which starts now`,
  "clusterBackups.plan.location": "Set where backups go",
  "clusterBackups.plan.schedule": "Set the backup schedule",
  "clusterBackups.plan.pause": "Pause backups",
  "clusterBackups.plan.resume": "Resume backups",
  "clusterBackups.plan.now": "Back up now",
  "clusterBackups.plan.create": (what: string) => `Adds ${what}.`,
  "clusterBackups.plan.patch": (what: string) => `Changes ${what}.`,
  "clusterBackups.plan.delete": (what: string) => `Removes ${what}.`,
  "clusterBackups.plan.nothing": "Nothing to change.",
  "clusterBackups.plan.keysHidden": "The keys go into a secret and aren't shown here.",
  "clusterBackups.thing.Secret": "the storage keys",
  "clusterBackups.thing.BackupStorageLocation": "where backups go",
  "clusterBackups.thing.Schedule": "the schedule",
  "clusterBackups.thing.Backup": (name: string) => `backup ${name}, which starts now`,
  "clusterBackups.thing.Namespace": (name: string) => `project ${name}`,
  "clusterBackups.thing.route": (namespace: string, name: string) =>
    `route ${name} in ${namespace}`,
  "clusterBackups.kube.namespace": "Velero's namespace",
  "clusterBackups.kube.location": "Backup location",
  "clusterBackups.kube.schedule": "Schedule",
  "clusterBackups.kube.keys": "Keys",
  "clusterBackups.kube.fs": "Volume files",
  "clusterBackups.project.label": "Cluster backups",
  "clusterBackups.project.lastFailed": (ago: string) =>
    `The last backup, ${ago}, didn't finish. Open backups to see why.`,
  "clusterBackups.project.last": (ago: string) =>
    `Last backed up ${ago}, with the cluster's backups.`,
  "clusterBackups.project.waiting": "In the cluster's backups. None has run yet.",
  "clusterBackups.project.notCovered": "Not in the cluster's backups.",
  "clusterBackups.project.noVelero": "The cluster has no backups set up.",
  "clusterBackups.project.open": "Open backups",
  "notify.title": "Notifications",
  "notify.what": "the notification channels",
  "notify.clusterIntro":
    "Alerts about nodes, Falco's runtime-security events and system namespaces go to these channels. Each project sets its own in its settings.",
  "notify.notInstalled": "Notifications need alerting",
  "notify.notInstalledBody":
    "Alerts come from the metrics component (VictoriaMetrics), which watches builds, rollouts, crashes, databases, backups, volumes and certificates. Install it from Components, then come back here to add a channel.",
  "notify.install": "Install metrics",
  "notify.verdict.none": "No channels yet. Alerts aren't sent anywhere.",
  "notify.verdict.rejected": (n: number) =>
    n === 1
      ? "Alertmanager rejected a channel. Its alerts aren't sent."
      : `Alertmanager rejected ${n} channels. Their alerts aren't sent.`,
  "notify.verdict.waiting": (n: number) =>
    n === 1
      ? "Waiting for Alertmanager to load the channel."
      : `Waiting for Alertmanager to load ${n} channels.`,
  "notify.verdict.ok": (n: number) =>
    n === 1 ? "1 channel receives alerts." : `${n} channels receive alerts.`,
  "notify.noRules":
    "Zeabur's alert rules aren't installed, so channels receive only alerts from other rules. Reinstall the metrics component to add them.",
  "notify.add": "Add channel",
  "notify.addTitle": "Add a notification channel",
  "notify.test": "Send test",
  "notify.testSent": (name: string) => `Test sent to ${name}. It arrives in a minute or two.`,
  "notify.more": (name: string) => `More actions for ${name}`,
  "notify.edit": "Edit",
  "notify.kept": "Unchanged",
  "notify.keptHint": "Leave empty to keep the saved value.",
  "notify.topicWithServer": "Enter the topic too; it is saved with the server.",
  "notify.remove": "Remove",
  "notify.unmanaged": "Made outside Zeabur. Change it where it was made.",
  "notify.events": "Events",
  "notify.noEvents": "Choose at least one event.",
  "notify.name": "Name",
  "notify.nameHint": (max: number) =>
    `Up to ${max} lower-case letters, digits and hyphens, starting with a letter.`,
  "notify.nameProblem": "Use lower-case letters, digits and hyphens, starting with a letter.",
  "notify.sendTo": "Send to",
  "notify.optional": (label: string) => `${label} (optional)`,
  "notify.required": "Required.",
  "notify.chatIdProblem": "Enter a number, such as -1001234567.",
  "notify.secretNote":
    "URLs and tokens go into a secret in this namespace. Zeabur doesn't show them again.",
  "notify.status.active": "Active",
  "notify.status.rejected": "Rejected",
  "notify.status.waiting": "Waiting",
  "notify.fix.secretMissing":
    "The channel's saved URL or token is gone. Edit the channel and enter it again.",
  "notify.fix.scheme":
    "The URL must start with https:// or http://. Edit the channel and enter the full URL.",
  "notify.fix.url": "The URL is not complete. Edit the channel and enter the full URL.",
  "notify.fix.required": "A required setting is empty. Edit the channel and fill it in.",
  "notify.fix.port":
    "The mail server needs a port, such as smtp.example.com:587. Edit the channel and add it.",
  "notify.kind.slack": "Slack",
  "notify.kind.discord": "Discord",
  "notify.kind.telegram": "Telegram",
  "notify.kind.email": "Email",
  "notify.kind.webhook": "Webhook",
  "notify.kind.ntfy": "ntfy",
  "notify.kind.other": "Other",
  "notify.field.webhookUrl": "Webhook URL",
  "notify.field.botToken": "Bot token",
  "notify.field.chatId": "Chat ID",
  "notify.field.to": "To",
  "notify.field.from": "From",
  "notify.field.smarthost": "SMTP server",
  "notify.field.username": "SMTP user",
  "notify.field.password": "SMTP password",
  "notify.field.url": "URL",
  "notify.field.bearerToken": "Bearer token",
  "notify.field.server": "Server",
  "notify.field.topic": "Topic",
  "notify.field.accessToken": "Access token",
  "notify.hint.slack": "From an incoming webhook of your Slack app. It starts with https://.",
  "notify.hint.discord": "From the channel's settings → Integrations → Webhooks.",
  "notify.hint.ntfyServer": "Leave empty for ntfy.sh.",
  "notify.hint.smarthost": "Host and port, such as smtp.example.com:587. TLS is required.",
  "notify.hint.chatId": "A number. Group chats start with -100.",
  "notify.event.build-failed": "Build failed",
  "notify.event.build-succeeded": "Build succeeded",
  "notify.event.deploy-failed": "Deploy failed",
  "notify.event.deploy-succeeded": "Deploy succeeded",
  "notify.event.crash-loop": "Crash loop",
  "notify.event.out-of-memory": "Out of memory",
  "notify.event.database-unhealthy": "Database unhealthy",
  "notify.event.backup-failed": "Backup failed",
  "notify.event.volume-almost-full": "Volume almost full",
  "notify.event.certificate": "Certificate expiring or not issued",
  "notify.event.node-not-ready": "Node not ready",
  "notify.event.runtime-security": "Runtime security (Falco)",
  "notify.plan.add": (name: string) => `Add channel ${name}`,
  "notify.plan.edit": (name: string) => `Edit channel ${name}`,
  "notify.action.add": "Add channel",
  "notify.action.edit": "Save channel",
  "notify.action.events": "Save events",
  "notify.action.remove": "Remove channel",
  "notify.action.test": "Send test",
  "notify.plan.remove": (name: string) => `Remove channel ${name}`,
  "notify.plan.test": (name: string) => `Send a test to ${name}`,
  "notify.plan.create": (what: string) => `Adds ${what}.`,
  "notify.plan.patch": (what: string) => `Changes ${what}.`,
  "notify.plan.delete": (what: string) => `Deletes ${what}.`,
  "notify.plan.nothing": "Nothing to change.",
  "notify.plan.hidden": "The URL and tokens go into a secret and aren't shown here.",
  "notify.plan.testHow":
    "The test is a Job that does nothing for up to two minutes. The alert rules see it and send a test alert to this channel only.",
  "notify.thing.channel": (name: string) => `channel ${name}`,
  "notify.thing.secret": "the channel's URL and tokens",
  "notify.thing.job": "a test Job",
  "notify.kube.namespace": "Namespace",
  "notify.kube.channel": (name: string) => `Channel ${name}`,
  "notify.kube.reason": (name: string) => `Why ${name} was rejected`,
  "notify.kube.secrets": "URLs and tokens",
  "notify.kube.secretsValue": "Secret of the same name as each config",
  "notify.kube.test": "Send test",
  "notify.kube.testValue":
    "Creates a Job labeled zeabur.com/notification-test; the ZeaburNotificationTest rule fires for it",

  // Databases
  "add.button": "New",
  "add.inProject": (project: string) => `New in ${project}`,
  "add.image": "Docker image",
  "add.database": "Database",
  "add.advanced": "Advanced",
  "add.yaml": "From YAML…",
  "add.needsComponent": "Install its component first",
  "add.imageTitle": "Deploy an image",
  "add.imageBody": (project: string) =>
    `Runs a container image in ${project}. You review the objects before anything is created.`,
  "add.field.image": "Image",
  "add.field.port": "Port",
  "add.portHint": "Leave empty for a background service that takes no connections.",
  "add.portRead": (port: string) => `Port ${port}, read from the image.`,
  "add.portChoose": (ports: string, port: string) =>
    `The image declares ports ${ports}; using ${port}.`,
  "add.portDeclared": (ports: string) => `The image declares ${ports}.`,
  "add.portUse": (port: string) => `Use ${port}`,
  "add.portNoPlatform": (platforms: string) =>
    `The image is built only for ${platforms}, which this cluster's nodes can't run.`,
  "add.portUnreadable.not-found":
    "The registry has no such image. Check the name and tag, or enter the port.",
  "add.portUnreadable.unauthorized":
    "The registry needs credentials to read this image. Add them under Private registry, or enter the port.",
  "add.portUnreadable.other": "Unable to read the image's port. Enter the port it listens on.",
  "add.noTag": "Without a tag the registry serves latest, which is hard to track later.",
  "add.problem.image.empty": "Enter the image to run.",
  "add.problem.image.invalid": "An image reference, like ghcr.io/acme/web:1.4.",
  "add.problem.name.empty": "Give it a name.",
  "add.problem.name.invalid": "Lowercase letters, digits and hyphens, starting with a letter.",
  "add.problem.name.long": "At most 40 characters.",
  "add.problem.name.taken": "Something in this project already has this name.",
  "add.problem.port.invalid": "A port from 1 to 65535.",
  "add.problem.host.empty": "Enter the domain.",
  "add.problem.host.invalid": "A domain name, like web.example.com.",
  "add.publicUrl": "Public domain",
  "add.hostHint": "Point this domain's DNS at the cluster's ingress.",
  "add.https": (issuer: string) => `HTTPS, with a certificate from cert-manager (${issuer}).`,
  "add.httpOnly":
    "This URL is HTTP only. An administrator can name a cert-manager issuer to make it HTTPS.",
  "add.noPublicUrl": "Your role can't add a public domain in this project.",
  "add.more": "More settings",
  "add.moreSummary": "Variables, volume, size, replicas, a private registry and security.",
  "add.variables": "Variables",
  "add.variablesHint": "KEY=value lines, as in a .env file.",
  "add.variablesBad": (lines: string) => `Not KEY=value: line ${lines}.`,
  "add.variablesSecret": (name: string) => `Keep the values in secret ${name}`,
  "add.volume": "Volume",
  "add.volumePath": "Mount path",
  "add.volumeSize": "Volume size",
  "add.volumeRecreate":
    "With a volume the service runs one replica, and an update stops the old replica before the new one starts: the volume mounts on one node at a time.",
  "add.problem.volumePath": "An absolute path, like /data.",
  "add.replicas": "Replicas",
  "add.replicasFixed": "One, because of the volume.",
  "add.registry": "Private registry",
  "add.registryServer": "Registry",
  "add.registryUser": "Username",
  "add.registryPassword": "Password",
  "add.problem.registry": "Fill in the registry, username and password.",
  "add.noSecrets":
    "Your role can't create secrets in this project: variables go in the Deployment, and private registries aren't offered.",
  "add.noVolume": "Your role can't add volumes in this project.",
  "add.editYaml": "Edit as YAML",
  "add.planImage": (name: string) => `Deploy ${name}`,
  "add.viewer": (project: string) =>
    `You can view ${project} but not add services to it. That needs a developer or admin role.`,
  "add.emptyBody": "Pick what to run. You review the objects before anything is created.",
  "add.card.image": "Run a container image from any registry.",
  "add.card.postgresql": "A PostgreSQL cluster run by its operator.",
  "add.card.mongodb": "A MongoDB replica set run by its operator.",
  "add.card.mysql": "A MySQL cluster run by its operator.",
  "add.card.mariadb": "A MariaDB server run by its operator.",
  "add.card.dragonfly": "A Redis-compatible Dragonfly store run by its operator.",
  "add.orYaml": "You can also create from YAML.",
  "add.kubectl": "Use kubectl",
  "add.palette.image": (project: string) => `Deploy an image in ${project}`,
  "add.palette.database": (engine: string, project: string) => `New ${engine} in ${project}`,
  "add.git": "Deploy from Git",
  "add.compose": "Docker Compose",
  "add.template": "App template",
  "add.templatesTitle": "App templates",
  "add.templatesBody": (project: string) =>
    `Apps ready to deploy in ${project}. You review the objects before anything is created.`,
  "add.templateFilter": "Filter templates",
  "add.templateNoMatch": (query: string) => `No template matches “${query}”.`,
  "add.templateNeeds": (cpu: string, memory: string) => `Needs ${cpu} and ${memory} of memory`,
  "add.templateTightMemory": "Not enough memory free",
  "add.templateTightCpu": "Not enough CPU free",
  "add.templateMissing": (names: string) => `Install ${names} first`,
  "add.templateDeploy": (name: string) => `Deploy ${name}`,
  "add.templateBack": "All templates",
  "add.templateMissingBody":
    "This app runs on that component. Install it from Components, then come back.",
  "add.templateInstall": (name: string) => `Install ${name}`,
  "add.templateUnknown": (names: string) =>
    `Zeabur can't tell whether ${names} is installed. If it isn't, the review says so.`,
  "add.templateNoIssuer":
    "This app needs HTTPS, but Zeabur has no certificate issuer set. An administrator can set one with --cluster-issuer when starting Zeabur.",
  "add.templateRoom": (use: string, free: string) =>
    `The cluster has room for it: it needs ${use} of memory, and ${free} is free.`,
  "add.templateLowMemory": (use: string, free: string) =>
    `The cluster is low on memory for it: it needs ${use}, and ${free} is free. Make room or add a node first.`,
  "add.templateLowCpu": (use: string, free: string) =>
    `The cluster is low on CPU for it: it needs ${use}, and ${free} is free. Make room or add a node first.`,
  "add.templateGenerated": (secrets: string) =>
    `Passwords and keys are generated in your browser and kept in secret ${secrets}.`,
  "add.templateInputEmpty": "Fill this in.",
  "add.templateInputInvalid": (pattern: string) =>
    `Doesn't match the expected format (${pattern}).`,
  "add.templateInputDefault": (value: string) => `Left empty, it becomes ${value}.`,
  "add.templateOptional": "Optional",
  "add.templateRisks": (n: number) => `Before you deploy: ${plural(n, "thing", "things")} to know`,
  "add.templateRisksBody": "The template's author wrote these notes.",
  "add.templateRiskFix": "What to do",
  "add.composeTitle": "Import Docker Compose",
  "add.composeBody": (project: string) =>
    `Makes a service in ${project} for each service of a Compose file. You review the objects before anything is created.`,
  "add.composeFile": "Compose file",
  "add.composeHint":
    "Paste compose.yaml or upload it. Zeabur doesn't keep the file: once imported, the services are edited like any other.",
  "add.composeUpload": "Upload file",
  "add.composeRead": "Read file",
  "add.composeReading": "Reading…",
  "add.composeChanged": "The text changed. Read the file again to review it.",
  "add.composeEnvFile": (path: string) => `Contents of ${path}`,
  "add.composeEnvHint":
    "The Compose file reads its variables from this file. Leave it empty to skip them.",
  "add.composeUnset": (names: string) =>
    `Nothing sets ${names}, so it stays empty. Paste the .env file that sets it.`,
  "add.composeNoEnvFile": (paths: string) =>
    `${paths} wasn't given; its variables are left out. Add them under Variables after import.`,
  "add.composeServices": (count: number) => `Services (${count})`,
  "add.composeManaged": (engine: string) => `Create ${engine} run by its operator`,
  "add.composeManagedHint": (image: string) =>
    `Instead of ${image}. Other services' connection variables point at it.`,
  "add.composeManagedAll": (count: number) => `Replace all ${count} with operator-run services`,
  "add.composeDiff.version": (image: string, version: string) =>
    `${image} names no version the operator offers; it runs ${version}.`,
  "add.composeDiff.command": "Its command isn't used; the operator starts it.",
  "add.composeDiff.ports": (to: string, from: string) => `Listens on ${to}, not ${from}.`,
  "add.composeDiff.volumes": (paths: string, gib: number) =>
    `Its volume at ${paths} isn't used: it starts empty on a new ${gib} GiB volume. Data isn't copied.`,
  "add.composeDiff.variables": (names: string) =>
    `${names} aren't used: the operator makes its own credentials.`,
  "add.composeDiff.instances": (from: number, to: number) =>
    `Runs ${plural(to, "instance", "instances")}, not ${from}.`,
  "add.composeKept.notInstalled": (image: string, engine: string) =>
    `Runs ${image} as it is: the ${engine} operator isn't installed. Install it in Components to have it run by the operator.`,
  "add.composeKept.unknown": (image: string, engine: string) =>
    `Runs ${image} as it is: Zeabur can't tell whether the ${engine} operator is installed.`,
  "add.composeKept.role": (image: string) =>
    `Runs ${image} as it is: your role can't create databases in this project.`,
  "add.composeBuildOnly": "Not created: it has no image, only build:. Deploy it from Git instead.",
  "add.composePorts": (ports: string) => `Listens on ${ports}`,
  "add.composeVolumes": (paths: string) => `Volume at ${paths}, 1 GiB each; grow it later`,
  "add.composeWaits": (names: string) => `Starts once ${names} accept connections`,
  "add.composeSecurity":
    "The services use Compatible security, because most Compose images run as root.",
  "add.composeMention": (variable: string, service: string) =>
    `${variable} names ${service} in a form Zeabur doesn't rewrite. Check it after import.`,
  "add.composePlan": (count: number) => `Import ${plural(count, "service", "services")}`,
  "add.composeFinding.privileged":
    "privileged is left out: a container doesn't get full access to the node.",
  "add.composeFinding.hostNetwork":
    "network_mode: host is left out: the service has its own network address.",
  "add.composeFinding.hostPid":
    "pid: host is left out: the service can't see the node's processes.",
  "add.composeFinding.devices": "devices is left out: a container doesn't get the node's devices.",
  "add.composeFinding.capAdd": (capabilities: string) =>
    `cap_add (${capabilities}) is left out: a service can't get more Linux capabilities.`,
  "add.composeFinding.dockerSocket": (source: string) =>
    `${source} isn't mounted: the Docker socket controls the whole node, and nodes may not run Docker.`,
  "add.composeFinding.hostPath": (source: string, target: string) =>
    `${source} isn't mounted at ${target}: it is a folder on the Docker host. Put its files in a volume or a config file after import.`,
  "add.composeFinding.healthcheck":
    "healthcheck is left out: Zeabur checks that the first port accepts connections. Change the check in Settings.",
  "add.composeFinding.build": (image: string) =>
    `build: is left out and ${image} is pulled. If the image is only built locally, deploy it from Git instead.`,
  "add.composeFinding.udpPort": (port: string) =>
    `UDP port ${port} is left out: the service's Kubernetes Service carries TCP only.`,
  "add.composeFinding.sharedVolume": (volume: string) =>
    `Volume ${volume} is shared in Compose; here each service gets its own.`,
  "add.composeFinding.renamed": (from: string, to: string) =>
    `${from} is renamed ${to}, because names can only use lowercase letters, digits and hyphens. Services that reach it as ${from} need the new name.`,
  "add.gitTitle": "Deploy from Git",
  "add.gitBody": (project: string) =>
    `Builds a Git repository and runs it in ${project}. You review the objects before anything is created.`,
  "add.field.repository": "Repository",
  "add.problem.repository.empty": "Enter the repository's address.",
  "add.problem.repository.invalid": "An HTTPS or SSH address, like https://github.com/acme/web.",
  "add.field.branch": "Branch",
  "add.problem.branch": "A branch name, like main.",
  "add.branchDefault": "Default branch",
  "add.branchHint": "Leave empty to build the default branch.",
  "add.field.method": "Build with",
  "add.method.railpack": "Railpack",
  "add.method.dockerfile": "Dockerfile",
  "add.methodHint.railpack":
    "Detects the language and builds without a Dockerfile. Pick Dockerfile if the repository has one.",
  "add.methodHint.dockerfile":
    "Builds the Dockerfile in the root directory. Name another file under More settings.",
  "add.gitPortHint":
    "The port the app listens on, also passed as PORT. Leave empty for a background service.",
  "add.access": "Repository access",
  "add.access.public": "Public repository",
  "add.access.deployKey": "Deploy key",
  "add.access.deployKeyHint":
    "Zeabur makes an SSH key for you to add to the repository, read-only.",
  "add.access.token": "Access token",
  "add.access.tokenHint": "A token that can read the repository, used over HTTPS.",
  "add.source": "Source",
  "add.source.github": "GitHub",
  "add.source.url": "Git URL",
  "add.githubSearch": "Search repositories",
  "add.githubEmpty": "No repository matches.",
  "add.githubListed": (date: string) =>
    `List from ${date}. Missing a repository? Ask a cluster admin to open it to this project again.`,
  "add.githubAccess": "Read through the cluster's GitHub App. Nothing else to set up.",
  "add.problem.githubRepository": "Pick a repository.",
  "add.access.noSecrets":
    "Your role can't add secrets in this project, so only public repositories work.",
  "add.deployKeyMake": "Create deploy key",
  "add.deployKeyPublic": "Public key",
  "add.deployKeyAdd": "Add this key to the repository's deploy keys, without write access.",
  "add.deployKeyOpen": (host: string) => `Open deploy keys on ${host}`,
  "add.keyscan": "Run on your computer",
  "add.keyscanHint":
    "Run it on a network you trust. Zeabur then lists each key's fingerprint for you to compare with the ones the host's admin publishes.",
  "add.knownHosts": "Host keys (known_hosts)",
  "add.knownHostsHint":
    "Paste what the command prints. Zeabur Builder connects only to a host that holds one of these keys.",
  "add.hostKeys": "Host keys Zeabur Builder trusts",
  "add.hostKeysCheck":
    "Compare these fingerprints with the ones the Git host's admin publishes. Add the public key only when they match.",
  "add.problem.deployKey": "Create the deploy key and add it to the repository.",
  "add.token": "Token",
  "add.problem.token": "Enter the username and token.",
  "add.gitMoreSummary": "Root directory, variables, build variables, size and security.",
  "add.field.directory": "Root directory",
  "add.directoryHint":
    "The folder to build, for a repository with several apps. Empty: the whole repository.",
  "add.problem.directory": "A folder in the repository, like apps/web.",
  "add.field.dockerfile": "Dockerfile",
  "add.dockerfileHint": "Relative to the root directory. Empty: Dockerfile.",
  "add.problem.dockerfile": "A file in the repository, like docker/Dockerfile.prod.",
  "add.buildVariables": "Build variables",
  "add.buildVariablesHint":
    "KEY=value lines that only the build reads, such as a registry token. They are passed as secrets and are not kept in the image.",
  "add.buildVariablesDockerfile": "A Dockerfile reads one with RUN --mount=type=secret,id=KEY.",
  "add.gitAfter":
    "The first build starts when you apply. After that, Zeabur Builder checks the branch every minute, and each new commit goes live once it builds.",
  "add.planGit": (name: string) => `Deploy ${name} from Git`,
  "add.card.git": "Build a Git repository and run it.",
  "add.palette.git": (project: string) => `Deploy from Git in ${project}`,
  "add.gitMissing.builder": "Needs Zeabur Builder in this cluster.",
  "add.gitMissing.registry":
    "Needs a container registry: install one in Components, or set builds.registry in Zeabur's chart.",
  "add.gitMissing.keel": "Needs Keel for auto-deploy. Install it in Components.",
  "add.gitMissing.registrySetting":
    "Zeabur has no registry for builds: set builds.registry in its chart.",
  "add.upload": "Upload a folder",
  "add.card.upload": "Build a folder from your computer and run it.",
  "add.palette.upload": (project: string) => `Upload a folder in ${project}`,
  "add.planUpload": (name: string) => `Deploy ${name} from a folder`,
  "upload.title": "Upload a folder",
  "upload.body": (project: string) =>
    `Builds a folder from your computer and runs it in ${project}. You review the objects before anything is created.`,
  "upload.zone": "Folder to upload",
  "upload.drop": "Compress the folder to a .zip or .tar.gz first, then drop it here",
  "upload.dropNote":
    "A .tar.gz keeps file permissions and symlinks. A folder or a .zip also works but loses both; only files that start with #! stay executable.",
  "upload.dropActive": "Drop to use it",
  "upload.choose": "Choose folder",
  "upload.chooseArchive": "Choose .zip or .tar.gz",
  "upload.reading": "Reading and packing the files…",
  "upload.ready": (files: number, size: string) =>
    `${plural(files, "file", "files")}, ${size} packed`,
  "upload.left": (n: number) =>
    `${plural(n, "file", "files")} left out by .gitignore, .dockerignore or .git.`,
  "upload.change": "Choose another",
  "upload.problem.folder": "Choose an archive or a folder.",
  "upload.problem.empty": "The folder has no files to build.",
  "upload.problem.packed": (largest: string) =>
    `The folder packs to more than 256 MiB. Leave the largest out in .gitignore: ${largest}.`,
  "upload.problem.unpacked": (largest: string) =>
    `The files add up to more than 1 GiB. Leave the largest out in .gitignore: ${largest}.`,
  "upload.problem.archiveInvalid": "Unable to read this archive.",
  "upload.problem.archivePacked": "The archive is over 256 MiB.",
  "upload.problem.archiveBomb": "The archive holds more than 1 GiB once unpacked.",
  "upload.moreSummary": "Folder to build, variables, build variables, size and security.",
  "upload.directoryHint": "The folder to build inside the upload. Empty: the whole upload.",
  "upload.methodHint.railpack":
    "Detects the language and builds without a Dockerfile. Pick Dockerfile if the folder has one.",
  "upload.methodHint.dockerfile":
    "Builds the Dockerfile in the folder's top level. Name another file under More settings.",
  "upload.problem.directory": "A folder in the upload, like apps/web.",
  "upload.problem.dockerfile": "A file in the upload, like docker/Dockerfile.prod.",
  "upload.after":
    "The folder goes up as soon as you apply, and the first build starts once it is there. To build a new version, upload the folder again from the service.",
  "upload.keepOpen": "Keep this page open until the upload finishes.",
  "upload.stage.token": "Asking Zeabur Builder for permission to upload…",
  "upload.stage.sending": (sent: string, total: string) => `Uploading… ${sent} of ${total}`,
  "upload.stage.naming": "Starting the build…",
  "upload.failed.token": "Zeabur Builder did not allow the upload.",
  "upload.failed.sending": "The folder did not upload.",
  "upload.failed.naming": "The folder is uploaded, but the build did not start.",
  "upload.retry": "Try again",
  "upload.sent": (build: string) => `Uploaded. ${build} is building.`,
  "upload.start": "Upload",
  "overview.building": "The image is being built. The service starts when the build finishes.",
  "overview.seeBuild": "See the build",
  "db.title": "Databases",
  "db.new": "New database",
  "db.newBody":
    "An operator runs the database. Zeabur creates the objects it needs in this project, with your permissions.",
  "db.engine.postgresql": "PostgreSQL",
  "db.engine.mongodb": "MongoDB",
  "db.engine.mysql": "MySQL",
  "db.engine.mariadb": "MariaDB",
  "db.engine.dragonfly": "Dragonfly",
  "db.licence": (engine: string, licence: string) =>
    `${engine} is under ${licence}. The Business Source License does not allow offering it to others as a hosted database service.`,
  "db.field.engine": "Engine",
  "db.field.name": "Name",
  "db.field.version": "Version",
  "db.field.members": "Instances",
  "db.field.storage": "Storage per instance",
  "db.field.size": "Size",
  "db.size.small": "Small",
  "db.size.medium": "Medium",
  "db.size.large": "Large",
  "db.sizeDetail": (cpu: string, memory: string, limit: string) =>
    `Each instance reserves ${cpu} and ${memory} of memory, and may use up to ${limit}.`,
  "db.name.empty": "Give it a name.",
  "db.name.invalid": "Lowercase letters, digits and hyphens, starting with a letter.",
  "db.name.long": "At most 30 characters.",
  "db.name.taken": "A database of this name exists.",
  "db.noRedundancy": (backedUp: boolean) =>
    `One instance: nothing takes over if it fails, and its data is on one volume.${backedUp ? "" : " Set up backups."}`,
  "db.replicaSetHint": "With three instances, a replica set keeps running when one fails.",
  "db.rseq.set":
    "Zeabur changed a memory setting so that this MongoDB 8 can start. It may run a little slower.",
  "db.rseq.missing":
    "On some Linux versions, MongoDB 8 starts only with a memory setting changed. This MongoDB does not have it yet. With it, MongoDB may run a little slower.",
  "db.rseq.fix": "Change setting",
  "db.rseq.plan": (name: string) => `Change the memory setting of ${name}`,
  "db.rseq.said": (name: string) =>
    `${name} takes the new setting (GLIBC_TUNABLES=glibc.pthread.rseq=1) and restarts one instance at a time.`,
  "db.onOneNode": "This cluster has one node, so one instance.",
  "db.planCreate": (name: string) => `Create database ${name}`,
  "db.noOperator": "To create databases, install a database component.",
  "db.refused": (engine: string) => `You don't have permission to read ${engine} databases.`,
  "db.notInstalled": (engine: string) => `${engine} is not installed in this cluster.`,
  "db.empty": "No databases yet.",
  "db.notFound": "This database is not in the project.",
  "db.state.ready": "Ready",
  "db.state.progressing": "Starting",
  "db.state.degraded": "Degraded",
  "db.state.failed": "Failing",
  "db.state.paused": "Paused",
  "db.state.unknown": "Unknown",
  "db.membersReady": (ready: number, total: number) => `${ready}/${total} ready`,
  "db.tab.backups": "Backups",
  "db.delete": "Delete database",
  "db.delete.made":
    "Also deletes the passwords, backup settings and other objects Zeabur made for it. Backups already in the bucket stay.",
  "db.delete.kept": (name: string, by: string) =>
    `Keeps the secret ${name}, which ${by} still uses.`,
  "db.delete.unlisted": (label: string) =>
    `Zeabur could not list everything it made for this database, so some objects may stay. Remove them with kubectl delete -l ${label}.`,
  "db.settings": "Settings of this database",
  "db.computeHint": "How many instances run, and the CPU and memory each one gets.",
  "db.customSize": "Set by hand in the object; picking a size replaces it.",
  "db.storage.invalid": "A size such as 10 GiB.",
  "db.storage.shrink": (current: string) => `It can grow, not go below ${current}.`,
  "db.storage.operatorFixed": (engine: string) =>
    `The ${engine} operator does not grow its snapshot volume. Set the size when you create it.`,
  "db.reshape": (name: string) => `Change ${name}`,
  "db.tabs": "Database sections",
  "db.fact.engine": "Engine",
  "db.fact.version": "Version",
  "db.fact.members": "Instances",
  "db.fact.primary": "Primary",
  "db.fact.storage": "Storage",
  "db.fact.size": "Size",
  "db.fact.custom": "Custom",
  "db.fact.created": "Created",
  "db.fact.operator": "Operator says",
  "db.connect": "Connect",
  "db.inCluster": "Reachable from inside the cluster.",
  "db.noDatabase":
    "This cluster has no app database. Services connect as moco-writable, which can create one.",
  "db.appDatabase": "App database",
  "db.noAuth": (engine: string) =>
    `This ${engine} has no password: anything in the cluster can connect to it.`,
  "db.export": "Export",
  "db.exportHint": (tool: string) =>
    `Runs ${tool} on the primary with your terminal access and downloads the dump it makes.`,
  "connect.action": "Connect to a service",
  "connect.addToService": "Add a database connection",
  "connect.title": (database: string) => `Connect ${database} to services`,
  "connect.titleService": (service: string) => `Add a database connection to ${service}`,
  "connect.planTitle": (database: string, services: string) => `Connect ${database} to ${services}`,
  "connect.database": "Database",
  "connect.services": "Services",
  "connect.servicesHint": "Optional. Each gets the connection string as a variable.",
  "connect.noTargets": "No service in this project that you may change.",
  "connect.already": "Connected",
  "connect.variable": "Variable name",
  "connect.badName": "Letters, digits and _, not starting with a digit.",
  "connect.extras": (names: string) => `Also add ${names}`,
  "connect.adds": "Variables added",
  "connect.note.postgresql": (secret: string) =>
    `The values stay in secret ${secret}; the service only points to them.`,
  "connect.note.url": (secret: string, url: string, password: string) =>
    `The password stays in secret ${secret}; ${url} takes it from ${password} when the service starts.`,
  "connect.taken": (name: string, service: string) => `${service} already has ${name}.`,
  "connect.disconnect": "Disconnect",
  "connect.actionsFor": (name: string) => `Actions for ${name}`,
  "connect.disconnectTitle": (service: string, database: string) =>
    `Disconnect ${service} from ${database}`,
  "connect.disconnectNote": (database: string, secret: string) =>
    `Only the service's variables are removed. ${database} and its secret ${secret} stay.`,
  "connect.noApp": (database: string) =>
    `${database} has no app user with a generated password, so it cannot be connected here. Databases created in Zeabur have one.`,
  "connect.noDatabases": "No database in this project can be connected.",
  "connect.waitsReady": (services: string, database: string) =>
    `${services} switch to the new version once ${database} is ready; the old version keeps running until then.`,
  "connect.readBy": (services: string) => `Connected to ${services}.`,
  "connect.none": "Not connected to a service yet.",
  "connect.bucket": "Bucket",
  "connect.note.s3": (secret: string) =>
    `The keys stay in secret ${secret}; the service only points to them.`,
  "connect.noKey": (store: string) =>
    `${store} has no access key that Zeabur made, so it cannot be connected here. Object storage created in Zeabur has one.`,
  "s3.title": "Object storage",
  "s3.engine": "SeaweedFS",
  "s3.tabs": "Object storage sections",
  "s3.new": "New object storage",
  "s3.newBody":
    "The SeaweedFS Operator runs an S3-compatible store. Zeabur creates the objects it needs in this project, with your permissions.",
  "s3.planCreate": (name: string) => `Create object storage ${name}`,
  "s3.name.empty": "Give it a name.",
  "s3.name.invalid": "Lowercase letters, digits and hyphens, starting with a letter.",
  "s3.name.long": "At most 30 characters.",
  "s3.name.taken": "Object storage of this name exists.",
  "s3.field.storage": "Storage",
  "s3.grow": (name: string) => `Grow the storage of ${name}`,
  "s3.growSentence": (name: string, from: string, to: string) =>
    `Grow the volume of ${name} from ${from} to ${to}. The operator grows it in place, and its files stay.`,
  "s3.field.bucket": "Bucket",
  "s3.bucketHint": "Optional. Add more later.",
  "s3.bucketRule": "3 to 63 lowercase letters, digits and hyphens.",
  "s3.bucket.empty": "Give it a name.",
  "s3.bucket.invalid":
    "3 to 63 lowercase letters, digits and hyphens, starting and ending with a letter or digit.",
  "s3.bucket.taken": "This store already has a bucket of this name.",
  "s3.oneCopy":
    "One copy: each part runs once and the files are on one volume. Nothing takes over if its node fails.",
  "s3.noOperator": "To create object storage, install the SeaweedFS Operator.",
  "s3.refused": "You don't have permission to read object storage.",
  "s3.empty": "No object storage yet.",
  "s3.notFound": "This object storage is not in the project.",
  "s3.bucketCount": (count: number) => (count === 1 ? "1 bucket" : `${count} buckets`),
  "s3.delete": "Delete object storage",
  "s3.fact.parts": "Parts",
  "s3.fact.buckets": "Buckets",
  "s3.fact.user": "Access",
  "s3.endpoint": "Endpoint",
  "s3.region": "Region",
  "s3.accessKey": "Access key ID",
  "s3.secretKey": "Secret access key",
  "s3.buckets": "Buckets",
  "s3.newBucket": "New bucket",
  "s3.noBuckets": "No buckets yet.",
  "s3.usage": (size: string, objects: number) =>
    objects === 1 ? `${size}, 1 object` : `${size}, ${objects} objects`,
  "s3.noUsage": "Not counted yet",
  "s3.planBucket": (bucket: string, store: string) => `Create bucket ${bucket} in ${store}`,
  "s3.deleteBucket": (bucket: string) => `Delete bucket ${bucket}`,
  "s3.deletesFiles": (bucket: string) => `The files in ${bucket} are deleted with it.`,
  "s3.keepsFiles": (bucket: string) =>
    `Only the Kubernetes object is deleted. The files in ${bucket} stay in the store.`,
  "add.composeKept.roleObjectStorage": (image: string) =>
    `Runs ${image} as it is: your role can't create object storage in this project.`,
  "db.host": "Host",
  "db.port": "Port",
  "db.database": "Database",
  "db.username": "Username",
  "db.password": "Password",
  "db.uri": "Connection string",
  "db.reveal": "Reveal",
  "db.hide": "Hide",
  "db.inSecret": (secret: string) => `In secret ${secret}`,
  "db.revealFailed": "Could not read the secret.",
  "db.commands": "Commands",
  "db.command.describe": "The database object",
  "db.command.shell": "A shell in the database",
  "db.backup.none": "Not backed up",
  "db.backup.last": "Last backup",
  "db.backup.noRun": "No backup yet",
  "db.backup.setUp": "Set up backups",
  "db.backup.setUpBody":
    "Backups go to an S3-compatible bucket you provide. The keys are kept as a secret in this project.",
  "githubApp.title": "GitHub App",
  "githubApp.body":
    "Once it is set up, project members pick a GitHub repository to deploy, and every push builds on its own. No deploy keys or webhooks to set up.",
  "githubApp.what": "Setting up the GitHub App",
  "githubApp.none":
    "This cluster has no GitHub App yet. Create one on GitHub; it takes about a minute.",
  "githubApp.create": "Create GitHub App",
  "githubApp.field.name": "Name on GitHub",
  "githubApp.field.nameHint": "It must be unique on GitHub.",
  "githubApp.field.organization": "Organization",
  "githubApp.field.organizationHint": "Leave empty to create it under your own account.",
  "githubApp.field.webhook": "Webhook URL",
  "githubApp.field.webhookHint":
    "Only if Zeabur Builder's webhook receiver is on. Without it, Zeabur Builder checks GitHub for new commits every minute.",
  "githubApp.problem.name.empty": "Enter a name for the App.",
  "githubApp.problem.name.long": "A name can have up to 34 characters.",
  "githubApp.problem.webhook": "The URL starts with https://.",
  "githubApp.continue": "Continue on GitHub",
  "githubApp.install": "Install on GitHub",
  "githubApp.open": "Open on GitHub",
  "githubApp.pollsOnly": "No webhook: new commits are picked up within a minute.",
  "githubApp.installations": "Installed on",
  "githubApp.noInstallations":
    "Not installed on any account yet. Install it on the accounts or organizations that hold your repositories.",
  "githubApp.installationsProblem": "Couldn't read the installations from GitHub.",
  "githubApp.selection.all": "All repositories",
  "githubApp.selection.selected": "Selected repositories",
  "githubApp.everyProject": "Every project",
  "githubApp.noProject": "No project can use it yet",
  "githubApp.choose": "Choose projects",
  "githubApp.grantHint":
    "Members of these projects can deploy the repositories the App can read on this account.",
  "githubApp.plan.save": "Save the GitHub App",
  "githubApp.plan.grant": (account: string) => `Choose which projects can use ${account}`,
  "githubApp.plan.secret":
    "Saves the App's keys, which Zeabur Builder reads the repositories with.",
  "githubApp.plan.grants": "Updates which projects Zeabur Builder lets use the installation.",
  "githubApp.plan.project": (project: string) =>
    `Updates the repositories ${project} can pick from.`,
  "githubApp.plan.keysHidden": "The private key goes into a secret and isn't shown here.",
  "githubApp.warning.replacesApp":
    "This replaces the cluster's current GitHub App. Services built with the old App stop picking up new commits until you open the new App to their projects.",
  "githubApp.warning.noWebhook":
    "Without a webhook, Zeabur Builder checks GitHub for new commits every minute.",
  "githubApp.warning.wasEveryNamespace":
    "Every project could use this account before. After this, only the projects you chose can.",
  "githubApp.warning.removesNamespaces":
    "Projects you unchecked can no longer build from this account's repositories.",
  "githubApp.callback.title": "Finishing the GitHub App",
  "githubApp.callback.missing":
    "GitHub didn't send back a code. Go back to the cluster page and start again.",
  "githubApp.callback.back": "Back to the cluster",
  "s3d.title": "S3 destinations",
  "s3d.body":
    "Backup forms offer these. Choosing one copies its keys into the project. New keys saved here go to the copies you may change.",
  "s3d.save": "Save destination",
  "s3d.plan.copy": (name: string, namespace: string) =>
    `Update copied keys ${name} in ${namespace}`,
  "s3d.plan.keptDenied": (copies: string) =>
    `You cannot change ${copies}. They keep the old keys until someone who can saves the destination again.`,
  "s3d.plan.keptUnknown": (copies: string) =>
    `${copies} do not name their keys, so they keep the old keys.`,
  "s3d.plan.unsearched":
    "You cannot list secrets in every namespace, so copies of these keys elsewhere keep the old keys.",
  "s3d.add": "Add destination",
  "s3d.plan.save": (name: string) => `Save S3 destination ${name}`,
  "s3d.plan.remove": (name: string) => `Remove S3 destination ${name}`,
  "s3d.problem.taken": "This name is taken.",
  "s3d.pick": "S3 destination",
  "s3d.manual": "Enter manually",
  "s3d.copied": (name: string) =>
    `The plan copies the keys of ${name} here. New keys saved to ${name} go to this copy too.`,
  "s3d.denied": (namespace: string) =>
    `You may not read the keys (secrets in ${namespace}). Enter them yourself.`,
  "s3d.from": (name: string) => `Keys copied from ${name}`,
  "s3d.none":
    "Buckets in the cluster's S3 destinations can be picked here, without typing their keys.",
  "db.backup.endpoint": "Endpoint",
  "db.backup.bucket": "Bucket",
  "db.backup.path": "Folder",
  "db.backup.pathHint": "The database's name when left empty.",
  "db.backup.region": "Region",
  "db.backup.regionHint": "Only if your storage service asks for one.",
  "db.backup.accessKey": "Access key ID",
  "db.backup.secretKey": "Secret access key",
  "db.backup.frequency": "Schedule",
  "db.backup.daily": "Daily at 03:00 UTC",
  "db.backup.weekly": "Sundays at 03:00 UTC",
  "db.backup.keep": "Keep, in days",
  "db.backup.problem.endpoint": "An http:// or https:// address.",
  "db.backup.problem.bucket": "A bucket name: lowercase letters, digits, dots and hyphens.",
  "db.backup.problem.keys": "Both keys are needed.",
  "db.backup.problem.keep": "From 1 to 365 days.",
  "db.backup.plan": (name: string) => `Back up ${name}`,
  "db.backup.said": (name: string, weekly: boolean, where: string) =>
    `Back up ${name} ${weekly ? "every Sunday" : "every day"} at 03:00 UTC to ${where}.`,
  "db.backup.saidKeep": (days: number) => `Each backup is kept for ${plural(days, "day", "days")}.`,
  "db.backup.saidKeys": (secret: string) => `The access keys are saved as secret ${secret}.`,
  "db.backup.saidRestart": (name: string) => `${name} restarts to start backing up.`,
  "db.backup.saidNow": (name: string, where: string) => `Back up ${name} to ${where} now.`,
  "db.backup.now": "Back up now",
  "db.backup.starting": "Ready once the database restarts with backups",
  "db.backup.planNow": (name: string) => `Back up ${name} now`,
  "db.backup.destination": "Destination",
  "db.backup.endpointFact": "Endpoint",
  "db.backup.retention": "Keeps",
  "db.backup.retentionDays": (days: number) => plural(days, "day", "days"),
  "db.backup.retentionCount": (count: string) => `The last ${count} backups`,
  "db.backup.schedule": "Schedule",
  "db.backup.unscheduled": "No schedule",
  "db.backup.runs": "Backups",
  "db.backup.storeIdle": (store: string) =>
    `${store} exists, but this database does not send its backups there yet.`,
  "db.backup.pluginMissing":
    "Backing up PostgreSQL needs the Barman Cloud plugin for CloudNativePG.",
  "db.backup.walNote": "PostgreSQL also archives its write-ahead log there continuously.",
  "db.backup.keepsEverything":
    "Every backup is kept. Set a lifecycle rule on the bucket to delete old ones.",
  "db.backup.fixedFolder": (folder: string) => `MOCO writes to ${folder} in the bucket.`,
  "db.backup.startsEmpty":
    "With one instance, Dragonfly restarts from the bucket, which is empty: the data so far stays in the snapshot on its volume.",
  "db.backup.retentionHours": (hours: string) => `${hours} of backups`,
  "db.backup.snapshots":
    "Dragonfly writes a snapshot there on its schedule and when it stops. Zeabur cannot list them.",
  "db.restore.action": "New database from a backup point",
  "db.restore.title": (name: string) => `New database from a backup point of ${name}`,
  "db.restore.body":
    "Zeabur creates a new database with the same settings, starting from the backup. The original is not changed.",
  "db.restore.point": "Backup",
  "db.restore.latest": "Newest snapshot",
  "db.restore.name": "New database name",
  "db.restore.none": "No finished backup or point in time to restore from yet.",
  "db.restore.fromBackup": "A finished backup",
  "db.restore.fromTime": "A point in time",
  "db.restore.window": (from: string, to: string) =>
    `Any time from ${from} to ${to}, in your time zone.`,
  "db.restore.time": "Time, in your time zone",
  "db.restore.utc": (instant: string) => `That is ${instant} UTC.`,
  "db.restore.outside": "This time is outside the range that can be restored.",
  "db.restore.instant": (local: string, utc: string) =>
    `The new database holds the data as of ${local}, which is ${utc} UTC, not what was written after it.`,
  "db.restore.asOfTime":
    "The new database holds the data as of the time you choose, not what was written after it.",
  "db.restore.making": (name: string) =>
    `Creating ${name}. Once it is ready, you can point services to it here.`,
  "db.restore.ready": (name: string) =>
    `${name} is ready. Next, point the services that read the original to it. You review each change first.`,
  "db.restore.download": (name: string) =>
    `To download the data of this point, export it from ${name}'s page. Backup files cannot be downloaded directly yet.`,
  "db.restore.plan": (from: string, name: string) => `Restore ${from} into ${name}`,
  "db.restore.unchanged": (from: string) =>
    `${from} is not changed, and services keep reading it until you point them to the new database.`,
  "db.restore.asOf":
    "The new database holds the data as of the backup, not what was written after it.",
  "db.restore.copiedSecret": (from: string, into: string) =>
    `The backup holds the passwords in ${from}, so the new database gets its own secret ${into} with the same values. It keeps working after the original is deleted.`,
  "db.restored.from": (name: string, time: string) => `Restored from ${name}'s backup of ${time}`,
  "db.restored.copy": (name: string, time: string) =>
    `${name} was restored from this database's backup of ${time}`,
  "db.restored.point": "Point services here",
  "db.restored.pointTitle": (name: string) => `Point services to ${name}`,
  "db.restored.keep": (name: string) =>
    `${name} keeps running and is kept. Delete it when you no longer need it.`,
  "db.restored.readers": (name: string, services: string) => `Reading ${name}: ${services}.`,
  "db.restored.noReaders": (name: string) => `No service reads ${name}.`,
  "db.restored.waiting": (name: string) => `You can point services to ${name} once it is ready.`,

  // Usage and history
  "usage.title": "Current usage",
  "usage.history": "History",
  "usage.live": "Live",
  "usage.liveHint":
    "Approximate values from metrics-server, measured about every 15 seconds. Memory is the working set, the figure the kubelet acts on. The tick marks what each replica reserves (its request); the bar ends at its maximum (its limit) and is hatched when there is none.",
  "usage.cores": (n: string) => (n === "1" ? "1 core" : `${n} cores`),
  "usage.coresUnit": "cores",
  "usage.usedWord": (v: string) => `${v} used`,
  "usage.reservedWord": (v: string) => `${v} reserved`,
  "usage.maxWord": (v: string) => `max ${v}`,
  "usage.noMax": "no maximum",
  "usage.noReserved": "nothing reserved",
  "usage.acrossPods": (n: number) => `of ${n} replicas`,
  "usage.ofMax": (pct: string) => `${pct} of max`,
  "usage.sampledOf": (a: number, b: number) => `${a}/${b} replicas measured`,
  "usage.perPodTemplate": (n: number, cpu: string, memory: string) =>
    `Each replica (×${n}): CPU ${cpu}, memory ${memory} (reserved / max)`,
  "usage.replica": "Replica",
  "usage.notes": "Notes",
  "usage.nearLimitOne": (pod: string) =>
    `${pod} is close to its memory maximum. Going over restarts it.`,
  "usage.nearLimitMany": (n: number) =>
    `${n} replicas are close to their memory maximum. Going over restarts them.`,
  "usage.needsMetricsServer": "Live usage needs metrics-server.",
  "usage.install": "Install",
  "usage.whatLive": "Seeing live usage",
  "usage.unavailable":
    "metrics-server did not respond, so only reserved and maximum amounts are shown.",
  "usage.note.oom": "Out of memory",
  "usage.note.oomHint":
    "Last stopped with OOMKilled: it went over its memory limit and was restarted.",
  "usage.note.nearLimit": "Near memory max",
  "usage.note.nearLimitHint": (pct: string) =>
    `Uses ${pct} of its memory limit. Over the limit, the container is stopped and restarted (OOMKilled).`,
  "usage.note.cpuAtLimit": "At CPU max",
  "usage.note.cpuAtLimitHint": "Held at its CPU limit (throttled): slower, but not restarted.",
  "usage.note.overRequest": "Above reserved",
  "usage.note.overRequestHint":
    "Uses more memory than it reserves. When the node runs short of memory, replicas above what they reserve are evicted first.",
  "usage.note.noSample": "No sample yet",
  "usage.note.noSampleHint":
    "metrics-server measures running containers only; a new replica appears within about 30 s.",
  "usage.split": "Show",
  "usage.total": "Total",
  "usage.perReplica": "Per replica",
  "usage.range": "Range",
  "usage.range.1h": "1h",
  "usage.range.6h": "6h",
  "usage.range.24h": "24h",
  "usage.range.7d": "7d",
  "usage.rangeLong.1h": "hour",
  "usage.rangeLong.6h": "6 hours",
  "usage.rangeLong.24h": "24 hours",
  "usage.rangeLong.7d": "7 days",
  "usage.reservedLine": "reserved",
  "usage.maxLine": "max",
  "usage.chartLabel": (metric: string, range: string) => `${metric}, last ${range}`,
  "usage.noSampleShort": "No sample",
  "usage.replacedPods": (n: number) =>
    `${plural(n, "replaced replica", "replaced replicas")} (faded)`,
  "usage.markerOom": "Out of memory",
  "usage.markerRestart": "Restart",
  "usage.restartsUnknown": "Restarts not known",
  "usage.restartsUnknownHint":
    "The metrics store has no restart counts: it does not collect kube-state-metrics.",
  "usage.needsStore": "History needs a metrics store.",
  "usage.installVm": "Install VictoriaMetrics",
  "usage.chooseStore": "Choose another store",
  "usage.needsStoreHint": (service: string) =>
    `Zeabur looked for ${service}. Any Prometheus-compatible store works, such as Prometheus or VictoriaMetrics; choose its Kubernetes Service in Settings.`,
  "usage.whatHistory": "Seeing usage history",
  "usage.proxyHint":
    "Zeabur reads the metrics store through the Kubernetes API as you. Anyone allowed to do that can read every project's metrics, so usually only administrators have that permission.",
  "usage.storeUnavailable": "The metrics store did not respond.",
  "usage.noSeries": (name: string, range: string) => `No samples for ${name} in the last ${range}.`,
  "usage.noSeriesHint":
    "The store answered but holds nothing for these replicas: it may not collect this node's container metrics, or not keep data this long.",
  "usage.source": "From",
  "usage.stepSeconds": (n: number) => `a point every ${n} s`,
  "usage.stepMinutes": (n: number) => `a point every ${n} min`,
  "usage.tenancy": "Who can see this",
  "usage.tenancyHint":
    "Zeabur asks only for this project's replicas. The store itself is not split by project: anyone allowed to open it can read every project's metrics.",
  "usage.loadingChart": "Loading chart…",

  // Settings: the cluster's name
  "settings.clusterName": "Cluster name",
  "settings.clusterNameLabel": "Name",
  "settings.clusterNameHint":
    "Shown in the sidebar and on Zeabur's pages, so people with several clusters can tell this one apart. Nothing in the cluster changes.",
  "settings.clusterNameReset": "Use the default name",
  "settings.clusterNameLocal":
    "Zeabur runs on this computer and keeps no settings: the name is your kubeconfig's.",
  // Settings: the metrics store
  "settings.metricsStore": "Usage history",
  "settings.store": "History source",
  "settings.storeHint":
    "The Kubernetes Service that holds usage history, as namespace/name:port. Any store with the Prometheus HTTP API works. Zeabur reads it through the Kubernetes API with each person's own rights.",
  "settings.storeDefault": "Default",
  "settings.storeIsDefault": "(default)",
  "settings.storeChosen": "Set in Zeabur",
  "settings.storeState": "Status",
  "settings.storeState.available": "Responding",
  "settings.storeState.not-configured": "Not found",
  "settings.storeState.unreadable": "No access",
  "settings.storeState.unavailable": "Not responding",
  "settings.storeSave": "Save",
  "settings.storeReset": "Use the default",
  "settings.storeSaved": "Saved",
  "settings.storeWho": "Usually only cluster administrators can change this.",
  "settings.storeWhereLabel": "Where this is kept",
  "settings.storeWhere": (namespace: string) =>
    `Kept in the config file zeabur-settings in the namespace ${namespace}. Anyone who can change that config file can change this setting.`,
  "settings.storeDenied": "You can't change it.",
  "settings.storeLocal":
    "Zeabur runs on this computer and keeps no settings. Start it with --metrics-service namespace/name:port to use another store.",
  "settings.storeTenancy":
    "Anyone who can read the store can read every project's metrics. Grant access only to people who may see the whole cluster.",
  "settings.signIn": "Sign-in",
  "settings.signInLead": "A session ends after 12 hours, or after an hour without use.",
  "settings.teams": "Teams",
  "settings.teamsHint": "How many teams share this cluster.",
  "settings.teams.single": "One team",
  "settings.teams.multiple": "Several teams",
  "settings.teams.singleMeans":
    "An admin account's session ends after an hour without use, like everyone else's.",
  "settings.teams.multipleMeans": "An admin account's session ends after 15 minutes without use.",
  "settings.adminAccounts": "Admin accounts",
  "settings.adminAal3": "Sign in only with a device-bound passkey",
  "settings.adminAal3Means":
    "A device-bound passkey stays on a security key or on one device and is never synced. An admin account's session then also ends after 15 minutes without use. Regular accounts sign in as before.",
  "settings.adminAal3Trust": "How Zeabur knows",
  "settings.adminAal3TrustHint":
    "The device that holds a passkey reports whether it is synced. Nothing verifies this.",
  "settings.adminAal3NeedsPasskey":
    "To turn this on, first add a device-bound passkey to your account: one kept on a security key or this device.",
  "settings.adminAal3AddPasskey": "Add a passkey",
  // Account and members (Zeabur Identity)
  "account.title": "Account",
  "account.you": "You",
  "account.username": "User name",
  "account.role": "Role",
  "account.passkeys": "Passkeys",
  "account.backup":
    "Add a backup passkey on your phone, so losing one device doesn't lock you out.",
  "account.linked": "Linked accounts",
  "account.yours": "your account",
  "account.groups": "Groups",
  "account.access": "Project access",
  "account.clusterAdmin": "You're a cluster administrator, so you're Admin in every project.",
  "account.unlisted": "You can't list projects here, so your role in each isn't shown.",
  "account.noProjects": "No projects yet.",
  "account.unread": "Couldn't read your access",
  "account.unreadCount": (n: number) =>
    `Your access to ${n === 1 ? "1 project" : `${n} projects`} couldn't be read. Reload to try again.`,
  "account.roleIn": (role: string, n: number) =>
    `${role} in ${n === 1 ? "1 project" : `${n} projects`}`,
  "account.summary": (parts: string[]) => `You're ${new Intl.ListFormat("en").format(parts)}.`,
  "account.noRole": (names: string[]) => `No role in ${new Intl.ListFormat("en").format(names)}.`,
  "account.noRoleCount": (n: number, all: boolean) =>
    all
      ? `You have no role in ${n === 1 ? "the project" : `any of the ${n} projects`}.`
      : `No role in ${n} other projects.`,
  "account.filter": "Filter projects",
  "account.noMatch": "No projects match.",
  "account.role.admin": "Admin",
  "account.role.developer": "Developer",
  "account.role.viewer": "Viewer",
  "rights.needs": (role: string) => `Needs ${role} access`,
  "rights.needsRule": (rule: string) => `Your Kubernetes rights don't include ${rule}`,
  "account.means.admin": "Everything a Developer can, and gives others access.",
  "account.means.developer":
    "Everything a Viewer can, and reads logs, changes services, opens terminals and reads secrets.",
  "account.means.viewer": "Sees services and replicas.",
  "account.signIn": "Sign-in methods",
  "account.signInWhere":
    "Zeabur Identity manages your passkeys, linked accounts and sessions: add or remove a passkey there, or sign out other devices.",
  "account.manageSignIn": "Manage sign-in methods",
  "passkey.unnamed": "Passkey",
  "passkey.added": (date: string) => `Added ${date}`,
  "passkey.lastUsed": "Last used",
  "passkey.none": "No passkeys.",
  "passkey.remove": "Remove",
  "passkey.removed": "Passkey removed",
  "passkey.lastKept": "Your only passkey can't be removed. Add another first.",
  "passkey.removeTitle": (name: string) => `Remove ${name}?`,
  "passkey.removeOwn": (name: string) =>
    `You can no longer sign in with ${name}. Your other passkeys keep working.`,
  "passkey.removeOther": (name: string, member: string) =>
    `${member} can no longer sign in with ${name}.`,
  "passkey.sessionsStay": "Sessions it already started keep working until they end.",
  "passkey.removeAction": "Remove passkey",
  "linked.none": "No linked accounts.",
  "linked.remove": "Unlink",
  "linked.removed": "Account unlinked",
  "linked.removeTitle": (provider: string) => `Unlink ${provider}?`,
  "linked.removeOwn": (provider: string) =>
    `You can no longer sign in with this ${provider} account. Your passkeys keep working.`,
  "linked.removeOther": (member: string, provider: string) =>
    `${member} can no longer sign in with this ${provider} account.`,
  "link.copy": "Copy link",
  "link.done": "Done",
  "link.expires": (when: string) => `Works once, until ${when}.`,
  "link.once": "It isn't shown again.",
  "link.send": (user: string) =>
    `Send this link to ${user}. Whoever opens it first adds a passkey to ${user}'s account.`,
  "members.title": "Members",
  "members.everyone": "the members",
  "members.invite": "Invite",
  "members.denied":
    "You don't have the rights to invite or change members. Ask a cluster administrator.",
  "members.filter": "Filter members",
  "members.none": "No one can sign in yet.",
  "members.noMatch": "No members match.",
  "members.you": "(you)",
  "members.role.admin": "Admin account",
  "members.role.member": "Member",
  "members.adminHint": "Signs in with a passkey only, and has full rights to the cluster.",
  "members.disabled": "Disabled",
  "members.column.name": "Name",
  "members.column.role": "Role",
  "members.column.passkeys": "Passkeys",
  "members.column.lastSignIn": "Last sign-in",
  "members.sessions": "Sessions",
  "members.passkeys": (n: number) => `${n} ${n === 1 ? "passkey" : "passkeys"}`,
  "members.noPasskey": "No passkey",
  "members.noPasskeyHint": "Can't sign in until they open a setup link.",
  "members.onePasskeyHint": "One lost device locks them out. A second passkey is a backup.",
  "members.actions": (name: string) => `Actions for ${name}`,
  "members.newLink": "New setup link",
  "members.reset": "Reset after a lost device",
  "members.remove": "Remove member",
  "members.pending": "Pending setup links",
  "members.pendingNew": "New member",
  "members.pendingAdds": "Adds a passkey",
  "members.expires": (when: string) => `Expires ${when}`,
  "members.revoke": "Revoke",
  "members.revoked": "Link revoked",
  "members.notInstalled": "Zeabur Identity isn't installed",
  "members.notInstalledBody":
    "Members are the people who sign in to this cluster through Zeabur Identity.",
  "members.noIssuer":
    "Zeabur doesn't know Zeabur Identity's address, so it can't make setup links. Start Zeabur with --issuer-url.",
  "members.notRedeemed":
    "Zeabur Identity doesn't accept setup links yet: they work once it runs with --invitations.",
  "invite.title": "Invite a member",
  "invite.body":
    "Makes a one-time link that works for 7 days. Whoever opens it first adds a passkey and signs in as this member.",
  "invite.username": "User name",
  "invite.usernameHint": "Lower-case letters, digits, - and . only.",
  "invite.invalid": "Use lower-case letters, digits, - and . only, at most 63 characters.",
  "invite.existing": (user: string) =>
    `${user} is already a member: the link adds a passkey to their account.`,
  "invite.displayName": "Display name",
  "invite.optional": "(optional)",
  "invite.admin": "Admin account",
  "invite.create": "Create link",
  "invite.action": (name: string) => `Invite ${name}`,
  "members.plan.adds": (name: string) => `Adds ${name} as a member.`,
  "members.plan.keeps": (name: string) =>
    `${name} is already a member and keeps their access. The link adds a passkey to their account.`,
  "members.plan.project": (title: string, namespace: string) => `${title} (${namespace})`,
  "members.plan.gets": (name: string, roles: string) => `${name} gets ${roles}.`,
  "members.plan.raised": (name: string, projects: string) => `${name} can now change ${projects}.`,
  "members.plan.noProjects": (name: string) =>
    `${name} can sign in and see project names, nothing more.`,
  "members.plan.link":
    "Makes a setup link that works for 7 days. It's shown once, after you confirm.",
  "members.plan.nothing": "Nothing changes.",
  "members.plan.creates": (n: number) => `${n} new`,
  "members.plan.changes": (n: number) => `${n} changed`,
  "members.plan.deletes": (n: number) => `${n} deleted`,
  "members.plan.changed": "Changed",
  "members.plan.in": (namespace: string) => `in ${namespace}`,
  "members.plan.notShown": "Its contents aren't shown.",
  "roles.legend": "Access",
  "roles.projects": "Chosen projects",
  "roles.projectsHint":
    "Developer or Viewer in the projects picked below. Every member also sees node and project names.",
  "roles.developer": "Developer",
  "roles.viewer": "Viewer",
  "roles.developerHint":
    "Developers deploy, change variables, restart, read logs and open terminals, and can read secret values.",
  "roles.viewerHint":
    "Viewers see status and events. They can't read logs, change anything, open a terminal or read secret values.",
  "roles.filter": "Filter projects",
  "roles.noProjects": "There are no projects yet.",
  "roles.noMatch": "No projects match.",
  "roles.noneChosen": "No project picked: they can sign in and see project names, nothing more.",
  "roles.chosen": (n: number) => `${n} ${n === 1 ? "project" : "projects"} picked`,
  "roles.roleIn": (project: string) => `Role in ${project}`,
  "roles.none": "No projects",
  "roles.in": (role: string, projects: string) => `${role} in ${projects}`,
  "roles.inCount": (role: string, n: number) => `${role} in ${n} projects`,
  "roles.projectsHeading": "Projects",
  "roles.change": "Change access",
  "roles.title": (name: string) => `Access for ${name}`,
  "roles.save": "Save access",
  "roles.saved": (name: string) => `Saved access for ${name}`,
  "roles.losesAdmin": (name: string) =>
    `${name} becomes a regular account and loses full rights to the cluster now.`,
  "roles.losesProjects": (name: string, projects: string) =>
    `${name} loses access to ${projects} now.`,
  "roles.losesChanges": (name: string, projects: string) =>
    `${name} can no longer change ${projects}.`,
  "roles.openTerminals": "A terminal already open stays open until it ends.",
  "roles.becomesAdmin": (name: string) =>
    `${name} becomes an admin account, with full control of the cluster: every project, the nodes and the members. From then on, ${name} signs in with a passkey only.`,
  "reset.title": (name: string) => `New setup link for ${name}`,
  "reset.keep": (name: string) =>
    `Adds a passkey to ${name}'s account; nothing is removed. The link works once, for 24 hours.`,
  "reset.option":
    "Also remove their passkeys, linked accounts and sessions, for a lost or stolen device",
  "reset.consequence": (name: string, parts: string) =>
    `${name} loses ${parts} now. Until someone opens the new link, nobody can sign in as ${name}.`,
  "reset.consequenceNone": (name: string) =>
    `${name}'s other setup links stop working. Until someone opens the new link, nobody can sign in as ${name}.`,
  "reset.passkeys": (n: number) => `${n} ${n === 1 ? "passkey" : "passkeys"}`,
  "reset.linked": (n: number) => `${n} linked ${n === 1 ? "account" : "accounts"}`,
  "reset.sessions": (n: number) => `${n} ${n === 1 ? "session" : "sessions"}`,
  "reset.resetCreate": "Reset and create link",
  "remove.title": (name: string) => `Remove ${name}?`,
  "remove.consequence": (name: string) =>
    `${name} can no longer sign in: their passkeys, linked accounts, sessions and setup links are removed.`,
  "remove.stays": (user: string) =>
    `Projects and everything in the cluster stay. Role bindings Zeabur didn't make that name ${user} stay too.`,
  "remove.action": (name: string) => `Remove ${name}`,
  "remove.done": (name: string) => `Removed ${name}`,
  "sentence.account.lastPasskey": () => "This is your only passkey",
  "sentence.account.lastPasskey.fix": () => "Add another passkey first, then remove this one.",
  "sentence.members.resetSelf": () => "You can't reset your own account",
  "sentence.members.resetSelf.fix": () =>
    "Add a passkey from your account instead, or ask another admin.",
  "sentence.members.removeSelf": () => "You can't remove yourself",
  "sentence.members.removeSelf.fix": () => "Ask another admin to remove you.",
  "sentence.members.noIssuer": () => "Zeabur doesn't know Zeabur Identity's address",
  "sentence.members.noIssuer.fix": () => "Start Zeabur with --issuer-url.",
  "sentence.members.demoteSelf": () => "You can't take away your own admin rights",
  "sentence.members.demoteSelf.fix": () => "Ask another admin to change your access.",
  "sentence.settings.signIn.noDeviceBound": () =>
    "You have no device-bound passkey, so you couldn't sign in after this change.",
  "sentence.settings.signIn.noDeviceBound.fix": () =>
    "Add a passkey kept on a security key or this device, then try again.",
  "sentence.members.noProjectRoles": () => "Zeabur was installed without the member roles",
  "sentence.members.noProjectRoles.fix": () =>
    "Install the chart with memberRoles.enabled=true, or make the member an admin.",
  "sentence.members.notInstalled": () => "Zeabur Identity isn't installed",
  "sentence.members.notInstalled.fix": () =>
    "Install Zeabur Identity. If it's installed, start Zeabur with the namespace it runs in.",
  // Account and members end
  // Sentences the server names by key (problems; diagnoses, gaps, traces and incidents are spread in
  // above); `writeSentence` finds them under `sentence.<key>`.
  "sentence.problem.forbidden.fix.askForRule": (p: P) =>
    `Ask a cluster administrator for ${p.rule}, for example with a ClusterRole granting it and a ClusterRoleBinding to you.`,
  "sentence.problem.forbidden.fix.sendGrant": () =>
    "Send this to an admin. It grants only that permission, and only in this project.",
  "sentence.problem.planExpired.fix.again": (p: P) =>
    `Reviewed changes hold for ${p.minutes} minutes. Review them again.`,
  "sentence.problem.badRequest.fix.planToken": () =>
    "Review the changes again. They apply only on the server that checked them, with their token unchanged.",
  "sentence.problem.conflict.fix.review": () => "Review again to start from its current state.",
  "sentence.problem.conflict.fix.alreadyExists": () =>
    "Delete them first, or deploy into another project.",
  "sentence.problem.conflict.fix.waitForContainer": () =>
    "Wait until it runs; its logs and events say why it doesn't.",
  "sentence.problem.unavailable.fix.tryAgainSoon": () => "Try again in a moment.",
  "sentence.problem.logUnavailable.fix.noPreviousRun": () => "Read the current run's log instead.",
  "sentence.problem.logUnavailable.fix.notStarted": () =>
    "Wait for the container to start; the replica's events say what it is waiting for.",
  "sentence.problem.logUnavailable.fix.removed": () =>
    "Wait a moment and refresh. If it stays unavailable, read the current run's log. To keep logs past restarts, send them to a log store.",
  "sentence.problem.quotaExceeded.fix.scale": (p: P) =>
    p.object
      ? `Run fewer replicas of ${p.object}, reserve less for each, or ask an admin to raise the limit ${p.quota}.`
      : `Ask for less, free some of what the project uses, or ask an admin to raise the limit ${p.quota}.`,
  "sentence.problem.policyDenied.fix.change": () =>
    "Change what it names, or ask whoever runs the policy.",
  "sentence.problem.invalid.fix.ephemeralContainers": () =>
    "Ask a cluster administrator to turn on ephemeral containers (on by default since Kubernetes 1.23), or use an image with a shell.",
  "sentence.problem.conflict.fix.terminalImage": (p: P) =>
    `Ask a cluster administrator to let the nodes pull ${p.image}, or to set ${p.setting} in Zeabur's chart to an image they can pull.`,
  "sentence.problem.conflict.fix.terminalContainerStart": () =>
    "Read the replica's events for the reason; a policy may forbid the user it runs as.",
  "sentence.problem.badRequest.fix.toolsLocally": () =>
    "Run kubectl and helm in your own terminal; they use the same kubeconfig as Zeabur.",
  "sentence.problem.summary.bad-request": () => "The request isn't valid.",
  "sentence.problem.summary.unauthenticated": () => "The cluster didn't accept your sign-in.",
  "sentence.problem.summary.signed-out": () => "You're signed out.",
  "sentence.problem.summary.forbidden": (p: P) =>
    p.verb ? `You can't ${verbEn[p.verb] ?? p.verb} ${thingEn(p)}.` : "You don't have permission.",
  "sentence.problem.summary.not-found": (p: P) =>
    p.name ? `${p.name} doesn't exist.` : "It doesn't exist.",
  "sentence.problem.summary.log-unavailable": () => "This log can't be read.",
  "sentence.problem.summary.conflict": (p: P) =>
    p.name ? `${p.name} changed after you reviewed it.` : "It changed after you reviewed it.",
  "sentence.problem.summary.already-exists": (p: P) =>
    `${p.objects} already exist in project ${p.namespace}.`,
  "sentence.problem.summary.managed": (p: P) =>
    p.name ? `Another tool manages ${p.name}.` : "Another tool manages this.",
  "sentence.problem.summary.plan-expired": () => "These changes expired.",
  "sentence.problem.summary.system-namespace": (p: P) =>
    `${p.name ?? "This name"} belongs to Kubernetes itself.`,
  "sentence.problem.summary.invalid": (p: P) =>
    p.name ? `The cluster refused the change to ${p.name}.` : "The cluster refused the change.",
  "sentence.problem.summary.too-many-requests": () => "The cluster is busy.",
  "sentence.problem.summary.github": (p: P) => `GitHub didn't answer as expected: ${p.detail}`,
  "sentence.problem.github.fix": () =>
    "Try again. If it keeps failing, check that the cluster can reach GitHub.",
  "sentence.problem.summary.unavailable": () => "The cluster can't be reached.",
  "sentence.problem.summary.internal": () => "Something went wrong in Zeabur.",
  "sentence.problem.summary.quota-exceeded": (p: P) =>
    p.name
      ? `The change to ${p.name} would go over the project's limit.`
      : "This would go over the project's limit.",
  "sentence.problem.summary.policy-denied": (p: P) =>
    p.name
      ? `A policy in the cluster refused the change to ${p.name}.`
      : "A policy in the cluster refused this.",
  "sentence.problem.why.quota-exceeded": (p: P) =>
    p.resource
      ? `The project's limit ${p.quota} allows ${p.hard} of ${p.resource}, and ${p.used} is already used.`
      : `The project's limit ${p.quota} doesn't leave room for this.`,
  "sentence.problem.why.policy-denied": (p: P) =>
    p.policy ? `The policy ${p.policy} answered:` : "The cluster answered:",
  // Problems the page makes itself
  "sentence.page.noAnswer": () => "Zeabur didn't respond.",
  "sentence.page.status": (p: P) => `The request failed with HTTP ${p.status}.`,
  "sentence.page.reload": () => "Check that Zeabur is still running, then reload the page.",
  "sentence.page.followFailed": () => "Unable to follow the log.",
  "sentence.page.followOff": () => "Turn off Follow; the snapshot shows the cluster's answer.",
  "status.readyUnchecked": "Running",
  "origin.code": "Errors in the app",
  "origin.configuration": "Settings to change",
  "origin.cluster": "Caused by the cluster",
  "origin.upstream": "Caused outside the cluster",
  "origin.unknown": "Other problems",
  "origin.code.hint": "The app's own code or command stops it; its logs say why.",
  "origin.configuration.hint":
    "A setting of this service causes it: a variable, a limit, a check or an address.",
  "origin.cluster.hint": "The cluster under the service causes it: its nodes, quotas or room.",
  "origin.upstream.hint": "Something outside the cluster causes it, such as an image registry.",
  "origin.unknown.hint":
    "Zeabur hasn't matched these to a known cause; each row says what the cluster reports.",
  "fixer.clusterAdmin": "This needs a cluster administrator.",
  "fixer.clusterAdmin.lead": "This needs a cluster administrator: ",
  "diagnosis.copy": "Copy diagnosis",
  "diagnosis.copyFailed": "Could not copy the diagnosis. Try again.",
  "diagnosis.lastSeen": "Last seen",
  "trace.title": "Connection checks",
  "trace.reachable": "Reachable",
  "trace.unreachable": "Not reachable",
  "trace.unverified": "Not verified from inside the cluster",
  "browserCheck.checking": "Checking from your browser…",
  "browserCheck.reachable": "Reachable from your browser",
  "browserCheck.unreachable": "Not reachable from your browser",
  "browserCheck.unreachableHint":
    "Check the domain's DNS record and its certificate. The browser doesn't say which one failed.",
  "browserCheck.skipped": "Your browser can't check an http:// address from here.",
  "browserCheck.scope": "Checked from your own network, not from the whole internet.",
  "browserCheck.again": "Check again",
  "trace.howToFix": "How to fix",
  "trace.showChecks": "Show connection checks",
  "trace.hop.ok": "Passed",
  "trace.hop.failed": "Failed",
  "trace.hop.unknown": "Not checked",
  "status.uncheckedWhy": "Its containers started, but nothing checks that it answers requests.",
  "status.uncheckedHint":
    "No traffic check is set, so Running only means the container started, not that it can serve requests.",
  "status.addTrafficCheck": "Add a traffic check",
  "status.private": "Private",
  "rollout.title": "Rollout",
  "rollout.ready": (ready: number, desired: number) => `${ready} of ${desired} new replicas ready`,
  "rollout.older": (n: number) =>
    plural(n, "old replica still running", "old replicas still running"),
  "rollout.stuckLine": "Deployment stuck",
  "rollout.waitsLine": "The new release waits on this",
  "rollout.rollingOut": (name: string) => `${name} is rolling out`,
  "overview.alsoMeans": "This also means",
  "overview.alsoDown": (list: string, name: string) =>
    `${list} can't be reached until ${name} has a ready replica.`,
  "overview.privateAddress": "the private address",
  "trace.waiting": (name: string) => `Waiting for ${name} to be ready`,
  "trace.stopped": (name: string) => `${name} is scaled to 0`,
  "event.word.retrying": "Retrying",
  "event.word.checkFailed": "Check failed",
  "event.word.stopping": "Stopping",
  "event.word.failed": "Failed",
  "event.word.noRoom": "Waiting for room",
  "event.word.mountFailed": "Can't mount volume",
  "event.word.createFailed": "Can't create replica",
  "event.word.gaveUp": "Gave up",
  "event.word.evicted": "Moved off its machine",
  "event.word.outOfMemory": "Out of memory",
  "changes.title": "Deployment history",
  "changes.none": "No deployments yet",
  "changes.release": (revision: number) => `Release #${revision}`,
  "changes.lead.current": (revision: number) => `Running #${revision}`,
  "changes.lead.deployed": "deployed",
  "changes.lead.stop": ". ",
  "changes.lead.week": (releases: number, warnings: number) =>
    `Last 7 days: ${plural(releases, "deployment", "deployments")}, ${warnings === 0 ? "no warnings" : plural(warnings, "warning", "warnings")}.`,
  "changes.older": "Older",
  "changes.filter": "Filter older changes",
  "changes.when": "When",
  "changes.what": "Change",
  "changes.noMatch": "No change matches.",
  "settings.sections": "Settings of this service",
  "part.variables": "Variables",
  "part.networking": "Networking",
  "part.storage": "Storage",
  "part.compute": "Compute",
  "buildOptions.hint":
    "Where in the repository Zeabur Builder builds from, and the variables only the build can read.",
  "buildOptions.none": "No build variables yet. Add KEY=value lines below.",
  "buildOptions.fromSecret": (secret: string) => `Value in secret ${secret}`,
  "buildOptions.literal": "Value written in the Build",
  "buildOptions.remove": "Remove",
  "buildOptions.add": "Add or replace",
  "buildOptions.addHint": (secret: string) =>
    `KEY=value lines. The values go to secret ${secret}; the build reads them as secrets and does not keep them in the image.`,
  "part.build": "Build",
  "part.health-checks": "Health checks",
  "part.security": "Security",
  "part.tasks": "Scheduled tasks",
  "part.schedule": "Schedule",
  "tasks.hint":
    "Run a command from this service on a schedule, such as a nightly cleanup or a report.",
  "tasks.none": "No scheduled tasks",
  "tasks.noneHint": "Add one to run a command from this service at set times.",
  "tasks.another": "Run another command on a schedule",
  "tasks.add": "Add scheduled task",
  "tasks.paused": "Paused",
  "tasks.badName": (max: number) =>
    `Use up to ${max} lowercase letters, digits and hyphens, starting and ending with a letter or digit.`,
  "tasks.needCommand": "Enter the command to run.",
  "tasks.needSchedule": "Enter when it runs.",
  "tasks.badKeep": "Enter a whole number from 0 to 100.",
  "tasks.planTitle": (name: string, service: string) =>
    `Add scheduled task ${name} from ${service}`,
  "tasks.addTitle": (service: string) => `Add a scheduled task to ${service}`,
  "tasks.addHint": (service: string) =>
    `Each run starts a new container with ${service}'s image, variables and volumes as they are now. A later image change to ${service} doesn't reach the task.`,
  "tasks.command": "Command",
  "tasks.commandHint": "Runs in a shell, from the image's working folder.",
  "tasks.schedule": "Schedule",
  "tasks.scheduleHint": "Five cron fields: minute, hour, day of month, month, day of week.",
  "tasks.zone": "Time zone",
  "tasks.zoneHint": "Leave empty to use the cluster's clock, usually UTC.",
  "tasks.name": "Name",
  "tasks.container": "Container to copy",
  "tasks.overlap": "If the last run is still running",
  "tasks.overlap.forbid": "Skip the new run",
  "tasks.overlap.allow": "Run both",
  "tasks.overlap.replace": "Stop the last run and start the new one",
  "tasks.keepSucceeded": "Successful runs to keep",
  "tasks.keepFailed": "Failed runs to keep",
  "tasks.kube.object": "Creates",
  "tasks.kube.command": "Command",
  "tasks.kube.copied": "Copied from the service",
  "tasks.kube.copiedValue":
    "image, env, envFrom, volumeMounts and the volumes they use, resources, securityContext, serviceAccountName, imagePullSecrets, nodeSelector, affinity, tolerations",
  "tasks.runs": "Runs",
  "tasks.runTitle": (name: string) => `Run ${name} now`,
  "tasks.runNow": "Run now",
  "tasks.noRuns":
    "No runs yet. The first one starts at the next scheduled time, or now with Run now.",
  "tasks.run.running": "Running",
  "tasks.run.succeeded": "Succeeded",
  "tasks.run.failed": "Failed",
  "tasks.manual": "Started by hand",
  "tasks.allRuns": (n: number) => `Show all ${n} runs`,
  "tasks.pauseTitle": (name: string) => `Pause ${name}`,
  "tasks.resumeTitle": (name: string) => `Resume ${name}`,
  "tasks.scheduleBlocked": "Fix the schedule to save.",
  "tasks.settingsHint": "Changes apply from the next run.",
  "tasks.overlapSkips": "A run is still going, and this task is set to skip the new run.",
  "tasks.replaceStops": (runs: string) =>
    `A run is still going, and this task stops it for a new one: ${runs} stops first.`,
  "confirm.certificateIssuer": (host: string, issuer: string) =>
    `Get the certificate of ${host} from ${issuer}.`,
  "confirm.certificateOwn": (host: string) => `Use your own certificate for ${host}.`,
  "action.useCertificate": "Use certificate",
  "warn.httpOnlyWildcard": (host: string) =>
    `${host} is HTTP only: a wildcard certificate needs an issuer with DNS-01. Create one with kubectl, then choose it`,
  "warn.ownCertificate": (host: string, date: string) =>
    `${host} uses your certificate. It expires on ${date}; replace it before then`,
  "warn.issuerShared": (route: string, issuer: string, hosts: string) =>
    `${issuer} also signs the certificates of ${hosts}: Ingress ${route} names one issuer for all its domains`,
  "domainCert.open": (host: string) => `Certificate of ${host}`,
  "domainCert.gateway":
    "This domain's certificate comes from the cluster's Gateway. An administrator changes it there",
  "domainCert.title": (host: string) => `Certificate of ${host}`,
  "domainCert.planTitle": (host: string) => `Change the certificate of ${host}`,
  "domainCert.lead": "Choose who signs the HTTPS certificate, or upload your own.",
  "domainCert.nowIssuer": (issuer: string) => `Signed by cert-manager (${issuer}).`,
  "domainCert.nowOwn": (secret: string) => `Uses the certificate in secret ${secret}.`,
  "domainCert.nowNone": "No HTTPS certificate yet.",
  "domainCert.expires": (date: string) => `Expires on ${date}.`,
  "domainCert.from": "Certificate",
  "domainCert.fromIssuer": "From an issuer, renewed automatically",
  "domainCert.fromOwn": "My own certificate",
  "domainCert.issuer": "Issuer",
  "domainCert.defaultIssuer": (issuer: string) => `Default (${issuer})`,
  "domainCert.shared": (hosts: string) => `The same issuer also signs ${hosts}.`,
  "domainCert.needsDns": (issuer: string) =>
    `${issuer} can't sign a wildcard domain: that needs a DNS-01 issuer`,
  "domainCert.ownMoves": (route: string, hosts: string) =>
    `Ingress ${route} asks cert-manager for the certificates of ${hosts} too, so this domain moves to an Ingress of its own.`,
  "domainCert.ownKept": (route: string, hosts: string, first: string) =>
    `Ingress ${route} asks cert-manager for the certificates of ${hosts} too, so this domain has to move to an Ingress of its own. ${first} first.`,
  "domainCert.keptBy.password": (_host: string) => "Remove the password",
  "domainCert.keptBy.redirect": (host: string) => `Stop redirecting ${host}`,
  "domainCert.keptBy.maintenance": (host: string) => `End maintenance on ${host}`,
  "domainCert.certificate": "Certificate (PEM)",
  "domainCert.certificateHint": "The certificate first, then any intermediate certificates.",
  "domainCert.key": "Private key (PEM)",
  "domainCert.keyHint":
    "Paste an unencrypted key. Zeabur stores it in a secret and never shows it again.",
  "domainCert.badCert": "Paste a PEM certificate that starts with -----BEGIN CERTIFICATE-----",
  "domainCert.badKey": "Paste a PEM private key that starts with -----BEGIN PRIVATE KEY-----",
  "domainCert.upload": "Upload file",
  "domainCert.nothing": "The domain already uses this issuer.",
  "domainCert.kube.issuer": "Issuer",
  "domainCert.kube.own": "Own certificate",
  "domainCert.kube.ownValue": (route: string) =>
    `kubernetes.io/tls Secret, owned by Ingress/${route}`,
  "domainCert.kube.tls": "Where it's served",
  "domains.title": "Domains",
  "domains.lead": "A new service's URL is suggested as",
  "domains.none":
    "New services have no suggested URL, so people type the whole domain. Add a domain you own, then point its wildcard record at the cluster.",
  "domains.default": "Default",
  "domains.edit": "Edit",
  "domains.editLead":
    "New URLs go under these domains, the first one by default. A domain can be a subdomain such as apps.example.com or the domain itself. URLs already made keep their names.",
  "domains.suffix": (n: number) => `Domain ${n}`,
  "domains.add": "Add a domain",
  "domains.remove": (name: string) => `Remove ${name}`,
  "domains.invalid": (list: string) => `${list} isn't a domain name.`,
  "domains.save": "Save",
  "domains.saved": "Domains saved",
  "domains.reset": "Go back to the install's domains",
  "domains.state.pointsHere": "Points to this cluster",
  "domains.state.sslip": "No DNS setup needed",
  "domains.state.missing": "No record yet",
  "domains.state.elsewhere": "Points elsewhere",
  "domains.state.unknown": "Not confirmed",
  "domains.missing": (wildcard: string) =>
    `No ${wildcard} record yet, so URLs under this domain don't reach the cluster.`,
  "domains.moved": (from: string, to: string) =>
    `The server's address changed from ${from} to ${to}. This domain still points to ${from}, and so do URLs made under it.`,
  "domains.useNew": (suffix: string) => `Use ${suffix} for new URLs`,
  "domains.kube.stored": "Kept in",
  "domains.kube.default": "Install's domains",
  "domains.kube.issuer": "Certificates from",
  "issuers.title": "Certificates",
  "issuers.wildcardsBy": (issuer: string) =>
    `Wildcard domains can get certificates from ${issuer}.`,
  "issuers.noWildcards":
    "No issuer signs wildcard domains yet. Create a DNS-01 issuer with kubectl to use domains such as *.example.com.",
  "issuers.notInstalled": "cert-manager isn't installed, so domains get no HTTPS certificates.",
  "issuers.install": "Install it",
  "issuers.ready.ready": "Ready",
  "issuers.ready.notReady": "Not ready",
  "issuers.ready.unknown": "Checking",
  "issuers.default": "Default",
  "issuers.staging": "Staging",
  "issuers.wildcards": "signs wildcard domains",
  "issuers.noWildcard": "no wildcard domains",
  "issuers.kube.issuer": "Issuer",
  "issuers.kube.domain": "How a domain picks it",
  "domainAccess.open": (host: string) => `Redirect or password for ${host}`,
  "domainAccess.title": (host: string) => `Access to ${host}`,
  "domainAccess.planTitle": (host: string) => `Change access to ${host}`,
  "domainAccess.lead": "Send visitors to another domain, or ask them for a password first.",
  "domainAccess.goes": (host: string) => `When someone opens ${host}`,
  "domainAccess.serve": (service: string) => `Show ${service}`,
  "domainAccess.redirect": "Send them to another domain",
  "domainAccess.to": "Domain to send them to",
  "domainAccess.toHint": "The path stays the same, and the new address uses HTTPS.",
  "domainAccess.badTo": "Enter a domain, like example.com.",
  "domainAccess.sameTo": "Enter a domain other than this one.",
  "domainAccess.kind": "Redirect type",
  "domainAccess.permanent": "Permanent (301)",
  "domainAccess.temporary": "Temporary (302)",
  "domainAccess.permanentHint":
    "Browsers remember a permanent redirect. Use temporary until you are sure.",
  "domainAccess.password": "Ask for a password",
  "domainAccess.passwordHint":
    "Visitors enter a username and password before they see the service. Zeabur keeps only a hash of the password.",
  "domainAccess.passwordShared": (hosts: string) =>
    `The password also covers ${hosts}, because they share this Ingress.`,
  "domainAccess.gatewayNoPassword":
    "Gateway API has no filter that asks for a password, so this domain's HTTPRoute can't ask for one.",
  "domainAccess.gatewayMoves": (hosts: string) =>
    `A redirect applies to the whole HTTPRoute, so this domain moves to a route of its own. ${hosts} stay where they are.`,
  "domainAccess.passwordSet": "A password is set.",
  "domainAccess.change": "Change",
  "domainAccess.username": "Username",
  "domainAccess.newPassword": "Password",
  "domainAccess.badUser": "Enter a username without spaces or colons.",
  "domainAccess.badPassword": "Use at least 8 characters, and at most 72 bytes.",
  "domainAccess.nothing": "Nothing changed.",
  "domainAccess.redirects": "Redirects",
  "domainAccess.protected": "Password protected",
  "domainAccess.kube.annotation": "Middleware list",
  "domainAccess.kube.redirect": "Redirect",
  "domainAccess.kube.password": "Password",
  "domainAccess.kube.users": "Users",
  "domainAccess.kube.usersValue": (name: string) =>
    `Secret ${name}, key users: one htpasswd line with a bcrypt hash`,
  "domainPages.open": (host: string) => `Maintenance and error pages for ${host}`,
  "domainPages.title": (host: string) => `Maintenance and error pages for ${host}`,
  "domainPages.planTitle": (host: string) => `Change the pages of ${host}`,
  "domainPages.lead":
    "Show visitors a page of your own while the service is down or you are working on it.",
  "domainPages.maintenance": "Maintenance mode",
  "domainPages.maintenanceHint": (host: string, service: string) =>
    `Visitors of ${host} see the maintenance page, with status 503. ${service} keeps running.`,
  "domainPages.errorPages": "Show error pages",
  "domainPages.errorPagesHint": (service: string) =>
    `When ${service} doesn't answer (502, 503 or 504), visitors see your page instead of a bare error.`,
  "domainPages.shared": (hosts: string) =>
    `The error pages also cover ${hosts}, because they share this Ingress.`,
  "domainPages.gatewayNoErrorPages":
    "Gateway API has no filter for error pages, so this domain's HTTPRoute shows only the maintenance page.",
  "domainPages.gatewayMoves": (hosts: string) =>
    `Maintenance applies to the whole HTTPRoute, so this domain moves to a route of its own. ${hosts} stay where they are.`,
  "domainPages.pages": "Pages",
  "domainPages.pagesHint": "Every domain of the project shows the same pages.",
  "domainPages.noServer": "This project has no page server yet",
  "domainPages.noServerHint":
    "These pages come from the Error pages template. Turning either switch on or editing a page also deploys it here: a small nginx.",
  "domainPages.room": (use: string, free: string) =>
    `The cluster has room for it: idle, it uses about ${use} of memory, and ${free} is free.`,
  "domainPages.tightMemory": (use: string, free: string) =>
    `The cluster is low on memory for it: idle, it uses about ${use}, and ${free} is free. Make room or add a node first.`,
  "domainPages.tightCpu": (use: string, free: string) =>
    `The cluster is low on CPU for it: idle, it uses about ${use}, and ${free} is free. Make room or add a node first.`,
  "domainPages.page": "Page to edit",
  "domainPages.page.maintenance": "Maintenance",
  "domainPages.page.badGateway": "502 Bad gateway",
  "domainPages.page.serviceUnavailable": "503 Service unavailable",
  "domainPages.page.gatewayTimeout": "504 Gateway timeout",
  "domainPages.html": "HTML",
  "domainPages.tooLarge": "Keep each page under 64 KiB.",
  "domainPages.nothing": "Nothing changed.",
  "domainPages.inMaintenance": "Maintenance",
  "domainPages.withErrorPages": "Error pages",
  "domainPages.default.lang": "en",
  "domainPages.default.maintenance": (project: string) => `${project} is under maintenance`,
  "domainPages.default.badGateway": (project: string) => `${project} isn't responding`,
  "domainPages.default.serviceUnavailable": (project: string) =>
    `${project} isn't available right now`,
  "domainPages.default.gatewayTimeout": (project: string) => `${project} took too long to respond`,
  "domainPages.default.later": "We'll be back soon.",
  "domainPages.default.retry": "Please try again in a few minutes.",
  "domainPages.kube.server": "Server",
  "domainPages.kube.serverValue": (name: string) =>
    `Service ${name} and its config file, found by label zeabur.com/page-server`,
  "domainPages.kube.serverNew":
    "Error pages template: Deployment, Service, config files and NetworkPolicy error-pages, labelled zeabur.com/page-server",
  "domainPages.kube.errors": "Error pages",
  "domainPages.kube.maintenance": "Maintenance",
  "domainPages.kube.maintenanceIngress": (route: string) =>
    `A second Ingress sends every request for the domain to the maintenance page. Its router.priority is higher than ${route}'s, and it is deleted with ${route}`,
  "domainPages.kube.maintenanceWeights": (route: string) =>
    `HTTPRoute ${route} sends every request to the maintenance port of the page server; its other backends get weight 0`,
  "ports.title": "Public ports",
  "ports.about":
    "Clients outside the cluster connect straight to a port, without a domain. For services that don't speak HTTP, such as a database.",
  "ports.open": "Open a port",
  "ports.none": "No port of this service is open to the internet.",
  "ports.waiting": (port: number) => `Port ${port} waits for an address from the load balancer.`,
  "ports.databaseOpen": "Database open to the internet",
  "ports.close": (port: number) => `Close port ${port}`,
  "ports.closeShort": "Close",
  "ports.closeTitle": (port: number, name: string) => `Close port ${port} of ${name}`,
  "ports.openTitle": (port: number, name: string) => `Open port ${port} of ${name} to the internet`,
  "ports.openTitleShort": (name: string) => `Open a port of ${name}`,
  "ports.databaseTitle": (name: string) =>
    `${name} is a database. Anyone on the internet can connect to it.`,
  "ports.databaseBody":
    "Its own password is then all that protects the data, and bots try common passwords on open database ports within minutes. When you can, connect from a service in this project through its private address instead.",
  "ports.anyoneTitle": "Anyone on the internet can connect to this port",
  "ports.anyoneBody":
    "Zeabur puts no password or HTTPS in front of it. The service must check who connects.",
  "ports.allOpen": "Every port of this service is already open.",
  "ports.port": "Port",
  "ports.portHint": "The same port number opens on every node's address.",
  "ports.kube.object": "Kubernetes object",
  "ports.kube.how": "How it opens",
  "ports.kube.howValue":
    "A LoadBalancer Service. On K3s, and on RKE2 with ServiceLB turned on (as Zeabur's installer sets it up), ServiceLB listens on the port on every node.",
  "ip.title": "IP limits",
  "ip.about":
    "Choose which addresses can open this service's domains, and which addresses its replicas can connect to.",
  "ip.noLimit": "No limit",
  "ip.limited": "Limited",
  "ip.some": "Limited on some domains",
  "ip.rules": "Limited by connection rules",
  "ip.in.title": "Who can open its domains",
  "ip.in.only": (list: string) => `Only ${list}`,
  "ip.in.noDomain": "Add a domain first.",
  "ip.in.gateway": "Domains on a Gateway can't be limited here.",
  "ip.out.title": "Where its replicas can connect",
  "ip.out.only": (list: string) => `Only ${list}, this project and DNS`,
  "ip.out.none": "Only this project and DNS",
  "ip.edit": "Edit",
  "ip.editIn": (name: string) => `Edit who can open ${name}`,
  "ip.editOut": (name: string) => `Edit where ${name} can connect`,
  "ip.in.drawer": (name: string) => `Who can open ${name}`,
  "ip.out.drawer": (name: string) => `Where ${name} can connect`,
  "ip.in.switch": "Only allow these addresses",
  "ip.out.switch": "Only allow connections to these addresses",
  "ip.addresses": "Addresses",
  "ip.in.hint": "One IP address or CIDR range per line, like 203.0.113.0/24.",
  "ip.out.hint":
    "One IP address or CIDR range per line. Leave it empty to allow only this project and DNS.",
  "ip.in.note":
    "Any other address gets 403. Behind a load balancer or proxy, Traefik may see the proxy's address instead of the visitor's.",
  "ip.out.note":
    "Other services in this project and DNS stay reachable. The limit works only if the cluster's network plugin enforces NetworkPolicy.",
  "ip.bad": (list: string) => `Not an IP address or CIDR range: ${list}.`,
  "ip.needOne": "Enter at least one address.",
  "ip.nothing": "Nothing changed.",
  "ip.planIn": (name: string) => `Limit who can open ${name}`,
  "ip.planOut": (name: string) => `Limit where ${name} can connect`,
  "ip.kube.in": (route: string) =>
    `Middleware ${route}-ip-allow ipAllowList, listed first on Ingress ${route}`,
  "ip.kube.out": (name: string) => `NetworkPolicy ${name}, policyTypes Egress`,
  "shared.title": "Shared variables",
  "shared.lead":
    "Variables that several services read. A service reads new values when it restarts.",
  "shared.secret": "Secret",
  "shared.plain": "Plain text",
  "shared.secretHint": "Values stay hidden until someone allowed to read secrets shows them.",
  "shared.plainHint": "Anyone who can see the project can read the values.",
  "shared.usedBy": "Used by",
  "shared.unused": "No service uses it yet.",
  "shared.showAll": (n: number) => `Show all ${n}`,
  "shared.none": "No shared variables yet",
  "shared.noneHint": "Add a group of variables, then use it from each service's Variables tab.",
  "shared.another": "Add another group of variables",
  "shared.add": "Add shared variables",
  "shared.addTitle": "Add shared variables",
  "shared.addLead": "A group of variables that services of this project can read.",
  "shared.planTitle": (name: string) => `Add shared variables ${name}`,
  "shared.name": "Name",
  "shared.kind": "Store values as",
  "shared.values": "Variables",
  "shared.valuesHint": "One NAME=value on each line, as in a .env file.",
  "shared.badName": "Use lowercase letters, digits, hyphens and dots.",
  "shared.taken": (kind: string, name: string) => `A ${kind} named ${name} already exists.`,
  "shared.needValues": "Add at least one NAME=value line.",
  "shared.unread": (line: number) => `Line ${line} is not NAME=value.`,
  "shared.badKey": (key: string) => `${key} is not a valid name: use letters, digits, -, _ and .`,
  "shared.kube.object": "Creates",
  "shared.kube.label": "Label",
  "shared.kube.read": "Read by",
  "shared.kube.readValue": "envFrom (every key) or env[].valueFrom (one key) in each service",
  "useShared.open": "Use shared variables",
  "useShared.title": (service: string) => `Use shared variables in ${service}`,
  "useShared.lead":
    "The service reads the values from the shared group, so a change there reaches it on its next restart.",
  "useShared.planTitle": (name: string, service: string) => `Use ${name} in ${service}`,
  "useShared.group": "Shared variables",
  "useShared.which": "Which variables",
  "useShared.allHint": "Every key becomes a variable, including keys added later.",
  "useShared.one": "One key",
  "useShared.oneHint": "Read one key under a name you choose.",
  "useShared.key": "Key",
  "useShared.variable": "Variable name",
  "useShared.badVariable": "Use letters, digits and _, not starting with a digit.",
  "useShared.exists": (name: string) => `The container already has ${name}.`,
  "useShared.already": (container: string) => `${container} already reads every key of it.`,
  "vars.removeReference": (name: string, source: string) => `Remove ${name}, read from ${source}`,
  "vars.removeReferenceSaid": (service: string, name: string) => `Remove ${name} from ${service}.`,
  "vars.referenceKept": (source: string) =>
    `${source} stays, with all its keys; only this variable goes.`,
  "useShared.stop": (name: string) => `Stop reading ${name}`,
  "useShared.kube.writes": "Adds",
  "useShared.kube.when": "Takes effect",
  "useShared.kube.whenValue": "When the pods restart (this change replaces them)",
  "warn.taskImageCopied": (service: string, image: string) =>
    `The task runs ${image}, ${service}'s image now; a later image change to ${service} doesn't reach it`,
  "warn.domainMoved": (host: string, from: string, to: string) =>
    `${host} moves from ${from} to a route of its own, ${to}`,
  "warn.routeDeleted": (route: string, host: string) =>
    `${route} routes only ${host}, so it is deleted with it`,
  "warn.permanentRedirect": (host: string) =>
    `Browsers remember a permanent redirect: if you remove it later, earlier visitors of ${host} still go to the old target until they clear their cache`,
  "warn.publicDatabase": (name: string) =>
    `${name} is a database: its own password is all that protects its data from the internet`,
  "warn.publicPortBlocked": (port: number, policies: string) =>
    `Port ${port} will not answer: connection rule ${policies} lets nothing from the internet in. Add an ingress rule with ipBlock 0.0.0.0/0 on port ${port} to it`,
  "warn.publicPort": (port: number) =>
    `Port ${port} opens on the nodes' addresses. If another program on a node already uses it, the port stays closed`,
  "warn.passwordOverHttp": (hosts: string) =>
    `${hosts} has no HTTPS, so the password crosses the network in plain text`,
  "form.healthHint":
    "Checks the cluster runs on each replica: whether it takes traffic, whether to restart it, and when it has started.",
  "draft.add": "Add to changes",
  "draft.pendingHere": "has changes waiting for review",
  "draft.count": (n: number) => plural(n, "change pending", "changes pending"),
  "draft.review": "Review changes",
  "draft.none": "No changes yet",
  "draft.save": "Save",
  "draft.leaveTitle": "Leave without saving?",
  "draft.leaveBody": (n: number) =>
    `${plural(n, "change has", "changes have")} not been applied. Leaving discards ${n === 1 ? "it" : "them"}.`,
  "draft.stay": "Stay",
  "draft.leave": "Discard and leave",
  "draft.discard": "Discard",
  "draft.title": (name: string) => `Save changes to ${name}`,
  "project.view": "View",
  "project.viewMap": "Map",
  "project.viewList": "List",
  "project.suggestions": (n: number) => plural(n, "suggestion", "suggestions"),
  "project.suggestionsBody":
    "Settings these services run without. None of them stops a service now, but each makes a problem easier to catch or avoid.",
  "project.activity": "Activity",
  "project.next.title": "Deploy the first service",
  "project.next.body":
    "A service shows up here as soon as a workload runs in this project: a Deployment, StatefulSet, DaemonSet, Job or CronJob.",
  "project.next.command": "Try it with a sample web server",
  "project.next.after": "Run it with kubectl, or apply your own YAML. The page updates by itself.",
  "freshness.failed": "Couldn't refresh",
  "freshness.justNow": "Updated just now",
  "freshness.ago": (count: number, unit: "minute" | "hour" | "day") =>
    `Updated ${count}${unit === "minute" ? "m" : unit === "hour" ? "h" : "d"} ago`,
  "freshness.refresh": "Refresh",
  "home.state.fine": (services: number, projects: number) =>
    `All ${services} services in ${plural(projects, "project", "projects")} are running.`,
  "home.state.problems": (n: number) =>
    n === 1 ? "1 problem needs attention." : `${n} problems need attention.`,
  "home.state.rolling": (n: number) =>
    n === 1 ? "1 service is rolling out." : `${n} services are rolling out.`,
  "home.state.node": (text: string) => `${text}.`,
  "home.state.unchecked": (n: number) =>
    n === 1 ? "1 project could not be checked." : `${n} projects could not be checked.`,
  "home.state.empty": "No projects yet.",
  "home.unchecked": (n: number) =>
    n === 1
      ? "1 project could not be checked; its problems are not listed here."
      : `${n} projects could not be checked; their problems are not listed here.`,
  "home.incidentsUnchecked":
    "Zeabur couldn't read all of the cluster's state, so a problem may be missing here.",
  "home.lastKnown": "Last known",
  "home.adoptTitle": "Existing namespaces",
  "home.showWarningsOf": (shown: number, total: number) => `Show ${shown} of ${total} warnings`,
  "home.allWarnings": (n: number) => `All ${n} on the cluster page`,
  "home.affects": "Affects",
  "home.inProgress": "In progress",
  "home.problems": "What needs you",
  "home.clusterNotices": (n: number) =>
    n === 1
      ? "1 cluster notice, nothing down because of it:"
      : `${n} cluster notices, nothing down because of them:`,
  "origin.component": "Caused by a cluster component",
  "origin.component.hint":
    "A component the cluster runs for every project causes it, such as the ingress controller or cluster DNS.",
  "origin.zeabur": "Caused by Zeabur",
  "origin.zeabur.hint": "Zeabur itself causes it. Report it with the copied diagnosis.",
  "fixer.componentOwner": "This needs the component's owner.",
  "fixer.componentOwner.lead": "This needs the component's owner: ",
  "incident.title": "In the cluster",
  "incident.hint": "One cause takes down several services; fixing it fixes them all.",
  "incident.domains": (n: number) => plural(n, "domain", "domains"),
  "incident.nodesNotReady": (n: number) => `${n} nodes are not ready`,
  "incident.nodesNotReporting": (n: number) => `${n} nodes stopped reporting to the cluster`,
  "incident.nodesNotReady.fix":
    "Check the machines behind them: whether they are on, their network, and their node agent (kubelet). Drain and replace one that won't come back.",
  "incident.services": (n: number) => plural(n, "service", "services"),
  "incident.servicesIn": (n: number, projects: number) =>
    `${plural(n, "service", "services")} in ${plural(projects, "project", "projects")}`,
  "incident.causedBy": "Caused by",
  "timeline.truncated": "Showing the newest 200 changes; older ones are left out.",
  "timeline.empty":
    "Nothing has changed lately. The cluster keeps events for about an hour and one write per tool.",
  "timeline.by": (who: string) => `by ${who}`,
  "timeline.tool.zeabur": "Zeabur",
  "timeline.tool.helm": "Helm",
  "timeline.tool.argoCd": "Argo CD",
  "timeline.tool.flux": "Flux",
  "timeline.tool.kubectl": "kubectl",
  "timeline.tool.autoscaler": "the autoscaler",
  "trace.step.dns": "DNS",
  "trace.step.address": "Entry address",
  "trace.step.controller": "Ingress controller",
  "trace.step.certificate": "Certificate",
  "trace.step.route": "Route",
  "trace.step.service": "Private address",
  "trace.step.targetPort": "Port",
  "trace.step.ready": "Ready replicas",
  "trace.expected": "Expected",
  "trace.saw": "Found",
  "trace.fix": "Fix it",
  "trace.fixTitle": (address: string) => `Fix the way to ${address}`,
  "rollout.rollBack": (name: string) => `Roll back ${name}`,
  "rollout.rollBackButton": "Roll back",
  "sentence.timeline.release": (p: P) =>
    `Release #${p.revision}${p.images ? ` runs ${p.images}` : ""}${p.changeCause ? ` (${p.changeCause})` : ""}.`,
  "sentence.timeline.write.zeabur": (p: P) => `Zeabur last changed ${p.fields || "it"}.`,
  "sentence.timeline.write.helm": (p: P) => `Helm last changed ${p.fields || "it"}.`,
  "sentence.timeline.write.argoCd": (p: P) =>
    `Argo CD (${p.manager}) last changed ${p.fields || "it"}.`,
  "sentence.timeline.write.flux": (p: P) => `Flux (${p.manager}) last changed ${p.fields || "it"}.`,
  "sentence.timeline.write.kubectl": (p: P) =>
    `kubectl (${p.manager}) last changed ${p.fields || "it"}.`,
  "sentence.timeline.write.autoscaler": () =>
    "The autoscaler (HorizontalPodAutoscaler) last changed the number of replicas.",
  "sentence.timeline.write.controller": (p: P) =>
    `Kubernetes (${p.manager}) last changed ${p.fields || "it"}.`,
  "sentence.timeline.write.other": (p: P) => `${p.manager} last changed ${p.fields || "it"}.`,
  "sentence.timeline.warning.crashLoop": (p: P) =>
    `${p.service} kept crashing and restarting (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.evicted": (p: P) =>
    `${p.service} was removed from its node (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.unschedulable.claimMissing": (p: P) =>
    `${p.service} couldn't start: the volume ${p.claim} it mounts doesn't exist (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.unschedulable.claimUnbound": (p: P) =>
    `${p.service} couldn't start: its volume wasn't ready (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.unschedulable.cpu": (p: P) =>
    `${p.service} waited for room: no machine had enough CPU free (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.unschedulable.memory": (p: P) =>
    `${p.service} waited for room: no machine had enough memory free (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.unschedulable.placement": (p: P) =>
    `${p.service} waited for a machine it is allowed to run on (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.unschedulable.other": (p: P) =>
    `${p.service} waited for a machine to run on (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.imagePull": (p: P) =>
    `${p.service} couldn't download its image (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.probe": (p: P) =>
    `${p.service} failed a health check (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.quota": (p: P) =>
    `${p.service} couldn't create a replica (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.other": (p: P) =>
    `${p.reason} on ${p.service} (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.termination.oomKilled": (p: P) =>
    `${containerEn(p)} ran out of memory and was stopped.`,
  "sentence.timeline.termination.exited": (p: P) =>
    `${containerEn(p)} ended with exit code ${p.exitCode}${p.reason ? ` (${p.reason})` : ""}.`,
  "sentence.rollout.progressing": (p: P) =>
    `${p.name} is rolling out: ${p.updated} of ${plural(Number(p.desired), "replica runs", "replicas run")} the new release.`,
  "sentence.rollout.done": (p: P) =>
    `${p.name} is rolled out: every replica runs the current release.`,
  "sentence.rollout.paused": (p: P) =>
    `The rollout of ${p.name} is paused; new replicas start once it is resumed.`,
  "sentence.rollout.waitsForDeletion": (p: P) =>
    `${p.name} replaces a replica only when that replica is deleted (OnDelete); ${p.updated} of ${p.desired} run the current release.`,
  "sentence.rollout.stuck.crashLoop": (p: P) =>
    `The rollout of ${p.name} is stuck: a new replica keeps crashing.`,
  "sentence.rollout.stuck.outOfMemory": (p: P) =>
    `The rollout of ${p.name} is stuck: a new replica runs out of memory.`,
  "sentence.rollout.stuck.imagePull": (p: P) =>
    `The rollout of ${p.name} is stuck: a new replica can't download its image.`,
  "sentence.rollout.stuck.unschedulable": (p: P) =>
    `The rollout of ${p.name} is stuck: a new replica is waiting for a machine to run on.`,
  "sentence.rollout.stuck.quota": (p: P) =>
    `The rollout of ${p.name} is stuck: the project's limit refuses new replicas.`,
  "sentence.rollout.stuck.configMissing": (p: P) =>
    `The rollout of ${p.name} is stuck: a new replica reads a secret or config that doesn't exist.`,
  "sentence.rollout.stuck.runFailed": (p: P) =>
    `The rollout of ${p.name} is stuck: a new replica's run failed.`,
  "sentence.rollout.stuck.podSecurity": (p: P) =>
    `The rollout of ${p.name} is stuck: security settings keep a new replica from starting.`,
  "sentence.fix.multiAttach": (p: P) =>
    `The old replica still holds its volume, and a volume attaches to one node at a time. In YAML, set ${p.service || "the service"}'s strategy.type to Recreate so the old replica stops first; each release then takes it offline for the seconds the new one needs to start.`,
  "sentence.fix.outOfMemory": (p: P) =>
    `Raise the memory maximum${p.limit ? ` above ${p.limit}` : ""}. Usage shows how close it gets. A maximum reserves nothing, so the node needs that much memory free.`,
  "sentence.fix.evicted.disk":
    "The node ran short of disk and stopped this replica. A new one starts on its own, without what the old one wrote outside a volume. Keep data on a volume and logs small, or ask a cluster administrator to free disk on that node.",
  "sentence.fix.evicted.memory":
    "The node ran short of memory and stopped this replica first. Reserve memory close to what it uses so it is stopped last. The reservation is held for it even when idle.",
  "sentence.fix.evicted":
    "The node ran short of resources and stopped this replica. A new one starts on its own. Its events say which resource ran short.",
  "sentence.rollout.stuck.deadlineExceeded": (p: P) =>
    `The rollout of ${p.name} is stuck: it made no progress before its deadline (progressDeadlineSeconds).`,
  "sentence.rollout.stuck.replicaFailure": (p: P) =>
    `The rollout of ${p.name} is stuck: the cluster can't create a new replica.`,
  // ia:start
  "preview.starts": "What starts",
  "preview.restarts": "What restarts",
  "preview.stops": "What stops",
  "preview.roll.rolling": (name: string) => `${name} replaces its replicas one at a time.`,
  "preview.roll.recreate": (name: string) => `${name} stops every replica, then starts new ones.`,
  "preview.roll.onDelete": (name: string) =>
    `${name} uses its new settings only when you delete its old replicas.`,
  "preview.scale": (name: string, from: number, to: number) =>
    `${name} goes from ${from} to ${plural(to, "replica", "replicas")}.`,
  "preview.start": (name: string, replicas: number) =>
    `${name} starts ${plural(replicas, "replica", "replicas")}.`,
  "preview.stop": (name: string) => `${name} stops.`,
  "preview.database": (name: string, engine: string, replicas: number) =>
    `The ${engine} database ${name} starts ${plural(replicas, "replica", "replicas")}.`,
  "preview.domain": (host: string) => `The domain ${host} starts serving.`,
  "preview.pendingRestart": (name: string) => `${name} keeps the old value until it restarts.`,
  "preview.downtime": "Nothing serves for a moment.",
  "preview.fits": (replicas: number, cpu: string, memory: string) =>
    `Fits: it adds ${plural(replicas, "replica", "replicas")}, ${cpu} and ${memory} at its peak.`,
  "preview.blockedQuota": (name: string, resource: string, need: string, left: string) =>
    resource === "pods"
      ? `Quota ${name} allows ${left} more replicas; this needs ${need}.`
      : `Quota ${name} allows ${left} more; this needs ${need}.`,
  "preview.blockedNodes": (_name: string, resource: string, need: string, left: string) =>
    resource === "pods"
      ? `No node has room for ${need} more replicas; the roomiest has room for ${left}.`
      : `No node has ${need} free; the roomiest has ${left}.`,
  "preview.fitUnknown": (replicas: number, cpu: string, memory: string) =>
    `It adds ${plural(replicas, "replica", "replicas")}, ${cpu} and ${memory} at its peak. Zeabur could not check whether they fit.`,
  "preview.dataLoss": "Data this loses",
  "preview.loss.claimDeleted": (claim: string, paths: string) =>
    `Deletes the volume ${claim}${paths ? ` and what is in ${paths}` : ""}.`,
  "preview.loss.emptyDir": (name: string, volume: string, paths: string) =>
    `${name} keeps ${paths || volume} in scratch space, which is emptied when its replicas are replaced.`,
  "preview.loss.statefulSet": (name: string, claim: string) =>
    `${name} deletes the volumes it made from ${claim}, with their data.`,
  "preview.removals": "Settings this removes",
  "preview.removal": (path: string, name: string) => `Removes ${path} from ${name}, now`,
  "preview.leftOut": "(left out of your edit)",
  "env.pendingRestart": "May still use the old value until restarted",
  "env.restartByItself": "Restart automatically",
  "env.pendingRestartDetail": (pod: string) =>
    `Changed after ${pod} started. Restart the service to use the new value.`,
  // ia:end
  // files:start
  "files.replica": "Replica",
  "files.replicaLabel": "Replica",
  "files.container": "Container",
  "files.loading": "Reading the files",
  "files.notRunningTitle": "No replica is running.",
  "files.notRunningBody":
    "Files are read through a running replica. Start one, or wait for the rollout; volumes keep their files meanwhile.",
  "files.refusedWhat": "browse files",
  "files.refusedRbac":
    "Files are read and written through pods/exec, the same right as the terminal. A cluster administrator can grant it in this project alone:",
  "files.roleLabel": "Role and RoleBinding",
  "files.noToolsTitle": "This image has no tar.",
  "files.noToolsBody":
    "Files are read and written with tar, as kubectl cp does, and distroless images leave it out. The volumes below come from the replica spec.",
  "files.addDebugBody":
    "A debug container (busybox) can be added to this replica to read its volumes, not the image's own files. It stays until the replica is replaced.",
  "files.addDebug": "Add a debug container",
  "files.addDebugWhat": "add a debug container",
  "files.debugStarting": "Starting the debug container…",
  "files.viaDebug": "The image has no tar; its volumes are read through debug container",
  "files.volumes": "Volumes",
  "files.location": "Location",
  "files.path": "Path",
  "files.what": "Kind",
  "files.keeps": "Kept",
  "files.part": "(part)",
  "files.kind.claim": "Disk",
  "files.kind.configMap": "Config file",
  "files.kind.secret": "Secret",
  "files.kind.emptyDir": "Scratch",
  "files.kind.image": "Image",
  "files.kind.other": "Volume",
  "files.kindHint.claim": "A disk: kept when replicas are replaced.",
  "files.kindHint.configMap": "Files from a config file. Kubernetes always mounts it read-only.",
  "files.kindHint.secret": "Files from a secret. Kubernetes always mounts it read-only.",
  "files.kindHint.emptyDir":
    "An emptyDir: kept while the replica runs, deleted when it is replaced.",
  "files.kindHint.image": "The image's own files, outside any volume.",
  "files.kindHint.other": "Another kind of volume.",
  "files.readOnly": "Read-only",
  "files.lostOnReplace": "Lost when replaced",
  "files.lostOnRestart": "Lost on restart",
  "files.notListed": "Needs tar",
  "files.imageUnreachable": "Not reachable from the debug container",
  "files.up": "Up one folder",
  "files.name": "Name",
  "files.size": "Size",
  "files.modified": "Modified",
  "files.actions": "Actions",
  "files.actionsFor": (name: string) => `Actions for ${name}`,
  "files.filter": "Filter by name",
  "files.upload": "Upload",
  "files.newFolder": "New folder",
  "files.download": "Download",
  "files.rename": "Rename",
  "files.delete": "Delete",
  "files.preview": "Preview",
  "files.empty": "Empty folder.",
  "files.items": (n: number) => (n === 1 ? "1 item" : `${n.toLocaleString("en")} items`),
  "files.shownOf": (shown: number, total: number) =>
    `First ${shown.toLocaleString("en")} of ${total.toLocaleString("en")}, by name.`,
  "files.shownOfMore": (shown: number, total: number) =>
    `First ${shown.toLocaleString("en")} of more than ${total.toLocaleString("en")}, by name.`,
  "files.shownOfHint": (limit: number) =>
    `A folder lists at most ${limit.toLocaleString("en")} entries. Filter by name to find the rest; the filter runs in the container.`,
  "files.configMapNotice": (name: string) =>
    `These files are the keys of config file ${name}. Change them in its editor.`,
  "files.secretNotice": (name: string) =>
    `These files are the keys of secret ${name}. Their contents stay hidden here.`,
  "files.edit": (name: string) => `Edit ${name}`,
  "files.reveal": (name: string) => `Reveal in ${name}`,
  "files.hidden": "hidden",
  "files.emptyDirNotice":
    "Scratch space: files here are deleted when this replica is replaced (a restart, a rollout or a move to another node).",
  "files.imageNotice":
    "The image's own files: what you change here is lost when the container restarts, and other replicas don't see it.",
  "files.tooLarge": (size: string) => `${size} is too large to preview. Download it instead.`,
  "files.binary": "Not a text file. Download it instead.",
  "files.create": "Create",
  "files.folderName": "Folder name",
  "files.newName": "New name",
  "files.in": "In",
  "files.renameTitle": (name: string) => `Rename ${name}`,
  "files.deleteTitle": (name: string) => `Delete ${name}`,
  "files.deleteConfirm": (name: string) => `Delete ${name}`,
  "files.deleteFile": (name: string, size: string) =>
    size ? `${name} (${size}) is deleted.` : `${name} is deleted.`,
  "files.deleteFolder": (name: string) => `${name} and everything in it are deleted.`,
  "files.deleteClaim": (claim: string) =>
    `It disappears from the volume (${claim}) at once, for every replica that mounts it. This can't be undone unless the volume is backed up.`,
  "files.deleteForever": "This can't be undone.",
  "files.target": "Path",
  "files.uploadTitle": (n: number) => (n === 1 ? "Upload 1 file" : `Upload ${n} files`),
  "files.uploadTo": "To",
  "files.replaces": "Replaces",
  "files.replaceWarning": (n: number) =>
    n === 1
      ? "A file with this name is already there; it is replaced."
      : `${n} files with these names are already there; they are replaced.`,
  "files.uploadLimit": (size: string) => `Up to ${size} per file.`,
  "files.uploadTooLarge": (size: string) =>
    `A file is larger than ${size}, the most Zeabur moves at once.`,
  "files.uploading": (n: number) => (n === 1 ? "Uploading 1 file" : `Uploading ${n} files`),
  "files.uploadsDone": (n: number) => (n === 1 ? "Upload finished" : `${n} uploads finished`),
  "files.uploaded": "Uploaded",
  "files.waiting": "Waiting",
  "files.failed": "Failed",
  "files.of": (done: string, total: string) => `${done} of ${total}`,
  "sentence.files.kernel": () => "/proc, /sys and /dev are kernel interfaces, not ordinary files.",
  "sentence.files.kernel.fix": () => "Read them in the terminal.",
  "sentence.files.imageUnreachable": (p: P) =>
    `${p.path} is in the image's own files, which the debug container doesn't reach.`,
  "sentence.files.imageUnreachable.fix": () => "Only the volumes can be read through it.",
  "sentence.files.secret": (p: P) => `This file is in secret ${p.secret}'s volume.`,
  "sentence.files.secret.fix": () => "Read its value in the secret's editor, after revealing it.",
  "sentence.files.configMapReadOnly": (p: P) => `These files come from config file ${p.object}.`,
  "sentence.files.configMapReadOnly.fix": () => "Change them in the config file's editor.",
  "sentence.files.secretReadOnly": (p: P) => `These files come from secret ${p.object}.`,
  "sentence.files.secretReadOnly.fix": () => "Change them in the secret's editor.",
  "sentence.databases.exportFailed": (p: P) => `The dump did not start: ${p.detail}`,
  "sentence.databases.exportFailed.fix": () =>
    "Check that the database is running, then try again.",
  "sentence.files.readOnly": (p: P) => `${p.path} is mounted read-only.`,
  "sentence.files.readOnly.fix": () => "Change the mount in Settings to write here.",
  "sentence.files.mountPoint": (p: P) => `${p.path} is or holds a mount point.`,
  "sentence.files.mountPoint.fix": () =>
    "A mount point can't be moved, renamed or deleted. Change the files inside it.",
  "sentence.files.noTools": (p: P) => `Container ${p.container} has no ${p.command}.`,
  "sentence.files.noTools.fix": () => "Add a debug container to read its volumes.",
  "sentence.files.notFound": (p: P) => `${p.path} isn't there.`,
  "sentence.files.notFound.fix": () => "It may have been moved or deleted; go up a folder.",
  "sentence.files.permissionDenied": (p: P) => `The container's user may not change ${p.path}.`,
  "sentence.files.permissionDenied.fix": () =>
    "The user the container runs as has no rights to this file. Run the image as a user that does, or change the volume's owner.",
  "sentence.files.readOnlyFileSystem": (p: P) => `${p.path} is on a read-only file system.`,
  "sentence.files.readOnlyFileSystem.fix": () => "Write to a volume instead.",
  "sentence.files.exists": (p: P) => `${p.path} already exists.`,
  "sentence.files.exists.fix": () => "Pick another name, or replace it.",
  "sentence.files.notADirectory": (p: P) => `A part of ${p.path} isn't a folder.`,
  "sentence.files.notADirectory.fix": () => "Go up a folder and look again.",
  "sentence.files.isADirectory": (p: P) => `${p.path} is a folder.`,
  "sentence.files.isADirectory.fix": () => "Open it instead.",
  "sentence.files.noSpace": () => "The volume is full.",
  "sentence.files.noSpace.fix": () => "Delete something, or make the volume larger in Settings.",
  "sentence.files.failed": (p: P) => `The container couldn't do this to ${p.path}.`,
  "sentence.files.failed.fix": (p: P) => `It said: ${p.message}`,
  "sentence.files.notAFile": (p: P) => `${p.path} isn't a file.`,
  "sentence.files.notAFile.fix": () => "Only files can be downloaded or previewed.",
  "sentence.files.tooLarge": (p: P) =>
    `The file is ${Math.ceil(Number(p.size) / 1e6)} MB, more than the ${Math.floor(Number(p.limit) / 1e6)} MB Zeabur moves at once.`,
  "sentence.files.tooLarge.fix": () => "Copy it with kubectl cp instead.",
  // files:end
}

export type Messages = typeof en
