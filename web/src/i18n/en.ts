// English: the source of every message key. The other locales are typed as `Messages`, so a key
// missing there (or added there only) is a TypeScript error.
import type { P } from "@/i18n/sentence-parts"
import sentences from "@/i18n/generated/en"

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`
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
const waitEn = (s: number) =>
  s < 60
    ? plural(s, "second", "seconds")
    : s < 3600
      ? plural(Math.ceil(s / 60), "minute", "minutes")
      : plural(Math.ceil(s / 3600), "hour", "hours")

export const en = {
  // Diagnoses, gaps, traces and incidents: crates/zeabur-gaps/messages
  ...sentences,
  // Shell
  "shell.skip": "Skip to content",
  "shell.nav": "Main",
  "shell.home": "Home",
  "shell.projects": "Projects",
  "shell.settings": "Settings",
  "shell.nameFrom.setting": "Name from Settings",
  "shell.nameFrom.install": "Name from the installation",
  "shell.nameFrom.kubeconfig": "Name from kubeconfig",
  "shell.nameFrom.address": "Name from the URL",
  "shell.nameFrom.node": "Name from the control plane node",
  "shell.nameFrom.provider": (provider: string) => `Name from ${provider}`,
  "shell.nameFrom.distributionNode": (distribution: string) =>
    `Name from the ${distribution} control plane node`,
  "shell.renameCluster": "Rename in Settings",
  "shell.cluster": "Cluster",
  "shell.search": "Search",
  "shell.jumpTo": "Jump to…",
  "shell.menu": "Menu",
  "shell.findProject": "Find a project",
  "shell.manage": "Manage",
  "shell.whoamiUnknown": "Unknown user",
  "shell.signOut": "Sign out",
  "shell.theme": "Theme",
  // Admin rights
  "elevate.retry": "Retry as admin",
  "elevate.title": "Admin rights",
  "elevate.waiting": "Confirm with your passkey in the new window.",
  "elevate.notAllowed": "This Zeabur may not ask for admin rights. Ask your cluster administrator.",
  "elevate.blocked": "The browser blocked the window. Allow pop-ups for this site, then try again.",
  "elevate.failed": "Admin rights were not granted.",
  "elevate.signedOut": "Your session ended. Sign in again.",
  "elevate.signIn": "Sign in",
  "elevate.until": (time: string) => `Admin until ${time}`,
  "elevate.activeDetail": "Everything you do uses your admin rights until then.",
  "elevate.drop": "Drop admin rights",
  "elevate.use": "Use admin rights",
  "shell.switchProject": "Switch project",
  "column.label": "Services",
  "column.other": "Other objects",
  "column.advanced": "Advanced",
  "column.namespace": "Namespace",

  "theme.system": "System theme",
  "theme.light": "Light",
  "theme.dark": "Dark",

  // Common
  "common.colon": ": ",
  "common.loading": "Loading…",
  "common.retry": "Try again",
  "common.all": "All",
  "common.close": "Close",
  "common.copy": "Copy",
  "common.copied": "Copied",
  "common.status": "Status",
  "common.sep": ", ",
  "common.list": (items: string[]) => new Intl.ListFormat("en").format(items),
  "common.more": (n: number) => `${n} more`,
  "common.domains": "Domains",
  "common.newTab": "(opens in a new tab)",
  "common.notFound": "This page doesn't exist",
  "common.backToProjects": "Back to projects",
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
  "problem.title.elevation-required": "This needs admin rights",
  "problem.title.quota-exceeded": "This would go over the project's limit",
  "problem.title.policy-denied": "A policy in the cluster refused this",
  "problem.title.not-found": "It does not exist",
  "problem.title.log-unavailable": "The log is not available",
  "problem.title.conflict": "It changed in the meantime",
  "problem.title.managed": "Another tool manages it",
  "problem.title.plan-expired": "The plan expired",
  "problem.title.system-namespace": "Kubernetes keeps this namespace for itself",
  "problem.title.invalid": "The cluster rejected the change",
  "problem.title.too-many-requests": "The cluster is busy",
  "problem.title.unavailable": "The cluster cannot be reached",
  "problem.title.internal": "Something went wrong in Zeabur",
  "problem.fix.bad-request": "Check the request and try again.",
  "problem.fix.unauthenticated": "Sign in again.",
  "problem.fix.signed-out": "Sign in again.",
  "problem.fix.forbidden": "Ask a cluster administrator for access, or use an account that has it.",
  "problem.fix.elevation-required": "Retry as admin, or ask a cluster administrator for access.",
  "problem.fix.quota-exceeded":
    "Ask for less, free some of what the project uses, or ask a cluster administrator to raise the limit.",
  "problem.fix.policy-denied":
    "Change what the policy names, or ask a cluster administrator about the policy.",
  "problem.fix.not-found": "Reload the page to see what the cluster has now.",
  "problem.fix.log-unavailable": "Read the current run's log, or wait a moment and refresh.",
  "problem.fix.conflict": "Reload to see the latest version, then make the change again.",
  "problem.fix.managed":
    "Change it where the managing tool reads it, such as Helm values or a Git repository. Restarting still works here.",
  "problem.fix.plan-expired": "Make the plan again.",
  "problem.fix.system-namespace": "Adopt a namespace of your own instead.",
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
    "Your Zeabur session ended, so Zeabur holds no sign-in to act with for you. Nothing is read or changed until you sign in again.",
  "problemPage.rule.forbidden":
    "The cluster decides what each person may do, by rules (RBAC) that say who may read or change which kinds of objects, in which projects. Zeabur acts only with your own rights: what they do not allow, it cannot do for you.",
  "problemPage.rule.elevation-required":
    "Your everyday rights do not allow this, but this Zeabur can give you admin rights for a short time once you confirm with your passkey. Everything you do then uses them, until they end or you drop them.",
  "problemPage.rule.quota-exceeded":
    "A project can have limits on what all its services may reserve together: CPU, memory, the number of replicas or objects. The cluster refuses a change that would go over them, whatever your rights.",
  "problemPage.rule.policy-denied":
    "A cluster can check every change against policies of its own, run by an admission webhook or a policy engine. When one refuses a change, its own words say why; your rights do not change its answer.",
  "problemPage.rule.not-found":
    "What was asked for is not in the cluster: it may have been deleted or renamed since the page read it.",
  "problemPage.rule.log-unavailable":
    "The cluster keeps a container's log for its current run and the run before, and only while its replica exists. A log never written, or one whose replica is gone, cannot be read.",
  "problemPage.rule.conflict":
    "Zeabur writes a change only onto the version you reviewed. When someone or something (a deploy tool, an autoscaler) changed the object after that, nothing is written, so no one's change is lost.",
  "problemPage.rule.managed":
    "Another tool (Helm, Argo CD, Flux) owns this object and puts its own version back at its next sync or upgrade. A change made here would be undone, so Zeabur refuses it.",
  "problemPage.rule.plan-expired":
    "A plan is what Zeabur will write, as you reviewed it. It holds for a few minutes only, so what is applied still matches the cluster as it is.",
  "problemPage.rule.system-namespace":
    "Kubernetes keeps some namespaces for its own components (kube-system, kube-public, kube-node-lease). They cannot become projects.",
  "problemPage.rule.invalid":
    "The cluster checks each object against its schema and rules before it stores it. It refused this one, and named the fields that are not valid.",
  "problemPage.rule.too-many-requests":
    "The cluster limits how many requests it serves at once, so it stays responsive. It turned this one away for now; nothing was changed.",
  "problemPage.rule.unavailable":
    "Zeabur could not reach the cluster's API server: it may be down or restarting, or out of reach from where Zeabur runs.",
  "problemPage.rule.internal":
    "Something failed inside Zeabur itself, not in the cluster. The details hold what Zeabur knows about it.",
  "refused.admin": "This needs admin rights.",
  "refused.adminFor": (what: string) => `${what} needs admin rights.`,
  "refused.rights": "This needs more rights.",
  "refused.rightsFor": (what: string) => `${what} needs more rights.`,
  "refused.ask": "Ask a cluster administrator.",
  "refused.view": "View as admin",
  "refused.details": "Details",
  "refused.missing": "Missing",
  "refused.see": (what: string) => `Seeing ${what}`,
  "refused.some": "Some of this page",
  "refused.terminal": "Opening a terminal",
  "refused.reveal": (name: string) => `Showing the values of ${name}`,
  "refused.listThese": "Listing these",
  "refused.adopt": (ns: string) => `Adopting ${ns}`,
  "refused.unadopt": (ns: string) => `Removing ${ns} from Zeabur`,

  // Status words
  "status.ready": "Ready",
  "status.rollingOut": "Rolling out",
  "status.failing": "Failing",
  "status.degraded": "Degraded",
  "status.scheduled": "Scheduled",
  "status.paused": "Paused",
  "status.rolloutStuck": "Rollout stuck",
  "status.failed": "Failed",
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
  "reason.imageName": "Image name is wrong",
  "reason.configMissing": "Missing config",
  "reason.cantStart": "Can't start",
  "reason.waitingForRoom": "Waiting for room",
  "reason.starting": "Starting",
  "reason.evicted": "Moved off its machine",
  "reason.releaseStuck": "Rollout stuck",
  "reason.gaveUp": "Gave up",
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
  "role.worker": "Workers",
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
  "cluster.attSkew.why": "Kubernetes does not support it, so pods there may misbehave.",
  "cluster.attSkew.fix": "Upgrade the kubelet to the control plane's minor version.",
  "cluster.attComponents": (n: number) => `${n} components are older than the tested versions`,
  "cluster.attComponent.why": "It still runs; Zeabur was tested with a newer version.",
  "cluster.attComponent.fix": "Upgrade it under Components.",
  "cluster.versionOf": (version: string, tested: string) => `${version}, tested ${tested}`,
  "cluster.showMembers": (n: number) => `Show all ${n}`,
  "cluster.notChecked":
    "Nothing found in what could be read, but part of the cluster could not be checked.",
  "cluster.cannotCheck": "Cannot check: the cluster cannot be reached.",
  "cluster.notRead": "Not read: the cluster cannot be reached.",
  "node.notReady": (n: number, node: string) =>
    n === 1 ? `${node} is not ready` : `${n} nodes are not ready`,
  "node.notReady.why": "Services with pods there are down until it is back or the pods move.",
  "node.notReady.fix":
    "Check that the machine is running and its kubelet can reach the control plane.",
  "node.memoryPressure": (n: number, node: string) =>
    n === 1 ? `${node} is out of memory` : `${n} nodes are out of memory`,
  "node.memoryPressure.why": "The kubelet is stopping pods to free memory.",
  "node.memoryPressure.fix":
    "Stop or limit the services that use the most memory, or add memory to the server.",
  "node.diskPressure": (n: number, node: string) =>
    n === 1 ? `${node} is almost out of disk space` : `${n} nodes are almost out of disk space`,
  "node.diskPressure.why":
    "The kubelet is removing unused images and stopping pods to free space; new pods cannot start.",
  "node.diskPressure.fix":
    "Free space: remove unused images (crictl rmi --prune), old logs or volumes, or grow the disk.",
  "node.pidPressure": (n: number, node: string) =>
    n === 1 ? `${node} is running out of processes` : `${n} nodes are running out of processes`,
  "node.pidPressure.why": "New processes and pods may fail to start.",
  "node.pidPressure.fix":
    "Find the service that starts too many processes, and restart or limit it.",
  "node.networkUnavailable": (n: number, node: string) =>
    n === 1 ? `${node} has no pod network` : `${n} nodes have no pod network`,
  "node.networkUnavailable.why": "Pods there cannot reach other pods or services.",
  "node.networkUnavailable.fix": "Check the network plugin on the node (flannel on k3s).",
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
  "node.diskFull.warn": "From 90% the kubelet stops pods to free space.",
  "node.diskFull.fail": "The kubelet is about to stop pods to free space.",
  "node.diskFull.fix":
    "Remove unused images (crictl rmi --prune), old logs or volumes, or grow the disk.",
  "node.cordoned": (n: number, node: string) =>
    n === 1 ? `${node} is cordoned` : `${n} nodes are cordoned`,
  "node.cordoned.why": "New pods are not placed there; the running ones stay.",
  "node.cordoned.fix": (n: number, node: string) =>
    n === 1
      ? `When its maintenance is done, uncordon it (kubectl uncordon ${node}).`
      : "When their maintenance is done, uncordon them.",
  "node.reservedMemory": (n: number, node: string, share: string) =>
    n === 1 ? `${node}: memory ${share} reserved` : `${n} nodes over-reserved on memory`,
  "node.reservedMemory.why": "New pods that ask for memory may wait for room.",
  "node.reservedMemory.fix": "Lower the memory requests of the services there, or add a node.",
  "node.reservedCpu": (n: number, node: string, share: string) =>
    n === 1 ? `${node}: CPU ${share} reserved` : `${n} nodes over-reserved on CPU`,
  "node.reservedCpu.why": "New pods that ask for CPU may wait for room.",
  "node.reservedCpu.fix": "Lower the CPU requests of the services there, or add a node.",
  "unreachable.title": (name: string) => `Cannot reach ${name}`,
  "unreachable.zeabur": "Zeabur does not respond",
  "unreachable.lastHeard": "Last answered",
  "unreachable.retrying": "Trying again…",
  "unreachable.every": "Zeabur tries again every 15 seconds.",
  "cluster.noProjects": "No projects yet",
  "cluster.noProjectsBody":
    "A project is a Kubernetes namespace shown in Zeabur. Adopt one on the Projects page.",
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
  "cluster.cores": (n: string) => `${n} cores`,
  "cluster.requestedOf": (requested: string, allocatable: string) =>
    `${requested} of ${allocatable} reserved`,
  "cluster.allocatable": (allocatable: string) => `${allocatable} allocatable`,
  "cluster.used": (used: string) => `${used} used`,
  "cluster.limits": (limits: string) => `${limits} maximum`,
  "cluster.podsOf": (placed: string, room: string) => `${placed} of ${room} pods`,
  "cluster.pods": "Pods",
  "cluster.worker": "worker",
  "cluster.kubelet": "kubelet",
  "cluster.skewMixed": "Supported, though its minor version differs from the control plane's.",
  "cluster.skewUnsupported":
    "Unsupported: newer than the control plane, or more than 3 minor versions older.",
  "cluster.noMetrics": "Install metrics-server to see usage.",
  "cluster.metricsUnavailable": "metrics-server did not respond.",
  "cluster.noUsageYet": "No usage measured yet.",
  "cluster.partialPods": (n: number) => `Reserved amounts count the first ${n} pods only.`,
  "cluster.unscheduled": (n: number) => `${n} pods waiting for a node`,
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
  "cluster.sourceK3s": "k3s",
  "cluster.sourceHelmChart": "HelmChart",
  "cluster.sourceHelm": "Helm",
  "cluster.sourceOther": "—",
  "cluster.roleReadsSecrets":
    "This role can read every Secret in the cluster: passwords, keys, and tokens. Grant it only to someone you would trust as a cluster administrator.",
  "cluster.copyRole": "Copy ClusterRole",
  "cluster.whatProjects": "the projects",
  "cluster.whatNodes": "the nodes",
  "cluster.whatRequests": "what pods reserve",
  "cluster.whatUpgrades": "the upgrade plans",
  "cluster.whatUsage": "node usage",
  "cluster.whatWarnings": "the cluster's warnings",
  "cluster.whatComponents": "the installed components",

  // Projects
  "projects.title": "Projects",
  "projects.adopted": "Project",
  "projects.namespaceOnly": "Namespace",
  "projects.other": "Other namespaces",
  "projects.noOther": "Every namespace is a project.",
  "projects.filterOther": "Filter namespaces",
  "projects.noOtherMatch": (query: string) => `No namespaces match “${query}”.`,
  "projects.noneBody":
    "A project is a Kubernetes namespace shown in Zeabur. Adopt one from the namespaces below.",
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

  // Architecture
  "lane.web": "Web",
  "lane.internal": "Private services",
  "lane.workers": "Workers & jobs",
  "lane.data": "Data",
  "edge.declared": "Declared",
  "edge.inferred": "Inferred",
  "edge.route": "Its public domain",
  "edge.sharedSecret": (name: string) => `Both read the secret ${name}`,
  "edge.sharedConfig": (name: string) => `Both read the config file ${name}`,
  "edge.shared": (kind: string, name: string) => `Both read ${kind} ${name}`,
  "edge.env": (variable: string) => `Named in the variable ${variable}`,
  "canvas.zoomIn": "Zoom in",
  "canvas.zoomOut": "Zoom out",
  "canvas.fit": "Fit to screen",
  "canvas.legend": "About the lines",
  "canvas.legendHint":
    "Drawn from Kubernetes objects and variable values, not from traffic. Declared: an Ingress or a shared Secret or ConfigMap. Inferred: a variable's value names the Service.",
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
  "tab.networking": "Networking",
  "tab.variables": "Variables",
  "tab.settings": "Settings",
  "tab.terminal": "Terminal",
  "tab.storage": "Storage",
  "tab.events": "Events",
  "tab.usage": "Usage",
  "ops.restart": "Restart",
  "ops.later": "Arrives in a later version.",
  "overview.started": (ago: string) => `Started ${ago}`,
  "overview.source": "Source",
  "overview.cluster": "Cluster",
  "overview.managedBy": "Managed by",
  "overview.replicas": (ready: number, desired: number) => `Replicas ${ready}/${desired}`,
  "overview.replicaSettings": "Replica settings",
  "overview.noneReady": "No replica is ready",
  "overview.someReady": (ready: number, desired: number) => `${ready} of ${desired} replicas ready`,
  "overview.crashing": (n: number) => plural(n, "replica keeps crashing", "replicas keep crashing"),
  "overview.manage": "Manage",
  "overview.warnings": "Warnings",
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
    "Reserved amounts and maximums from the pod template. Usage needs metrics-server.",
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
  "logs.replaced": (old: string, current: string) => `${old} was replaced; showing ${current}.`,
  "logs.gone": (pod: string) =>
    `${pod} no longer exists; looking for the replica that took its place.`,
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
  "dock.label": "Terminal",
  "dock.show": "Show terminal",
  "dock.hide": "Hide terminal",
  "dock.resize": "Terminal height",
  "dock.close": (name: string) => `Close ${name}`,
  "dock.count": (count: number) => (count === 1 ? "1 terminal" : `${count} terminals`),
  "dock.expires": (time: string) => `Credential expires at ${time}`,
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
  "terminal.adminEnded":
    "Admin rights ended, so the terminal closed. Reopen it to continue with your own rights.",
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
  "tools.expired":
    "The credential in this terminal has expired. Reopen it to continue with a fresh one.",
  "tools.reopen": "Reopen",

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
  "vars.secretsHint": "About Secret values",
  "vars.revealHint":
    "Zeabur fetches a value only when you reveal it, and keeps it on this page only.",
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
  "net.exposeHint": "To add one, apply a Service or run this command.",
  "net.noneTitle": (name: string) => `${name} can't receive connections`,
  "net.noneBody":
    "It has no private address and no public domain. A worker that only makes outgoing connections needs neither, and can still reach other services and the internet.",
  "net.scheduledTitle": (name: string) => `${name} doesn't receive traffic`,
  "net.scheduledBody":
    "A job runs and exits, so nothing connects to it. It can still connect to other services and the internet.",
  "reach.fromDomains": "Public domains",
  "net.anyHost": "Any host",
  "net.defaultBackend": "Default backend",

  // Storage
  "storage.title": "Volumes",
  "volume.fullestOf": (used: string, n: number) => `${used}, fullest of ${n} replicas`,
  "volume.full":
    "When it is full, the app's writes fail. Free space in it, or set a larger size and apply.",
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
  "storage.problem.badSize": "Enter a size such as 1Gi.",
  "storage.problem.shrinks": "A volume can grow but never shrink.",
  "storage.problem.badName": "Use lowercase letters, digits and -.",
  "storage.problem.nameTaken": "Another new volume has this name.",
  "storage.problem.badClaimName": "Use lowercase letters, digits, - and ., at most 253 characters.",

  // Palette
  "palette.title": "Command palette",
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
  "settings.language": "Language",
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
  "cap.dns": "DNS records",
  "cap.event-autoscaling": "Event autoscaling",
  "cap.ingress": "Ingress",
  "cap.intrusion-prevention": "Intrusion prevention",
  "cap.logs": "Log storage",
  "cap.metrics": "Metrics",
  "cap.node-updates": "Node updates",
  "cap.object-storage": "Object storage",
  "cap.observability-operator": "Monitoring operator",
  "cap.postgres": "PostgreSQL databases",
  "cap.resource-recommendations": "Resource recommendations",
  "cap.runtime-security": "Runtime security",
  "cap.uptime-probes": "Uptime probes",
  "cap.volume-backup": "Volume backups",
  "cap.vulnerability-scanning": "Vulnerability scanning",
  // Objects and YAML
  "project.entry.objects": "Objects",
  // A project's namespace
  "project.entry.settings": "Settings",
  "action.adopt": "Adopt",
  "action.unadopt": "Stop showing as a project",
  "action.openDebugTerminal": "Open debug terminal",
  "action.openToolsTerminal": "Open kubectl terminal",
  "action.openInToolsTerminal": "Open in kubectl terminal",
  "action.saveDetails": "Save details",
  "title.adopt": (ns: string) => `Adopt ${ns}`,
  "title.unadopt": (ns: string) => `Stop showing ${ns} as a project`,
  "title.details": (ns: string) => `${ns} details`,
  "title.pauseAll": (ns: string) => `Pause every service in ${ns}`,
  "title.restartAll": (ns: string) => `Restart every service in ${ns}`,
  "confirm.adopt": "Adds the project label to the namespace. Nothing in it changes.",
  "confirm.unadopt": "The namespace stops showing as a project. Nothing in it is deleted.",
  "confirm.debugContainer": (pod: string) =>
    `Adds a debug container with a shell to replica ${pod}. It can see the app's processes.`,
  "confirm.debugStays":
    "It stays in the replica until the replica is replaced, for example by a restart. Kubernetes cannot remove it sooner.",
  "confirm.debugReused":
    "A debug container already runs in this replica; the terminal opens in it.",
  "confirm.clusterTools":
    "Starts a temporary container with kubectl and helm for you, in this project.",
  "confirm.toolsActsAs": (person: string, admin: boolean) =>
    admin
      ? `kubectl and helm act as ${person}, with the admin rights you hold now.`
      : `kubectl and helm act as ${person}, with your own permissions.`,
  "confirm.toolsCredential": (minutes: number) =>
    `Your credential in it runs out in ${plural(minutes, "minute", "minutes")}; reopen the terminal for a fresh one.`,
  "confirm.toolsStops": (most: number, after: number) =>
    `It stops after ${plural(most, "minute", "minutes")} at most, and is deleted ${plural(after, "minute", "minutes")} after its last terminal closes.`,
  "confirm.toolsReused": "Your kubectl terminal is already running; it opens again.",
  "confirm.details": "Saves the name, description, and icon on the namespace.",
  "access.title": (ns: string) => `You can't change ${ns}`,
  "access.you": "<your user name>",
  "access.ask": (ns: string) =>
    `Ask a cluster administrator for permission to patch the namespace ${ns}.`,
  "access.askManage": (ns: string) =>
    `Ask a cluster administrator for permission to patch the namespace ${ns} and to zeabur-manage it. Without zeabur-manage, Zeabur's admission policy lets a patch change only the project's name, description and icon.`,
  "access.detailsOnly":
    "Zeabur's admission policy keeps this permission to the project's name, description and icon; the namespace's labels stay as they are.",
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
  "projectSettings.changeDetails": "Changing this project's details",
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
  "projectSettings.shownYes": "Yes. The namespace has Zeabur's project label.",
  "projectSettings.shownNo": "No. Adopting adds Zeabur's project label and changes nothing else.",
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
  "objects.scan": "Scan all kinds",
  "objects.scanning": "Scanning…",
  "objects.scanned": "All kinds read",
  "objects.refresh": "Refresh",
  "objects.load": (kind: string) => `Read ${kind}`,
  "objects.unread": "Not read",
  "objects.denied": "No access",
  "objects.pin": (kind: string) => `Pin ${kind}`,
  "objects.unpin": (kind: string) => `Unpin ${kind}`,
  "tab.yaml": "YAML",
  "yaml.hidden": "Secret values are hidden.",
  "yaml.label": (name: string) => `YAML of ${name}`,

  // Plans
  "plan.planning": "Planning…",
  "plan.apply": "Apply",
  "plan.applying": "Applying…",
  "plan.cancel": "Cancel",
  "plan.replan": "Review again",
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
      ? "The new pods wait as Pending until a node has room. The old ones keep running, so the service stays up, but the change isn't finished until then."
      : `The new pods wait as Pending until a node has room. ${names} ${n === 1 ? "doesn't" : "don't"} run until then.`,
  "plan.short.quota": (names: string, n: number) =>
    n === 0
      ? "The quota refuses the new pods until it has room. The old ones keep running, so the service stays up, but the change isn't finished until then."
      : `The quota refuses the new pods until it has room. ${names} ${n === 1 ? "doesn't" : "don't"} run until then.`,
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
  "risk.deletes": "Deleting can't be undone.",
  "warn.rightsMissing": (rights: string) => `The cluster may refuse this: you lack ${rights}.`,
  "warn.toolsCredentialExposed": (namespace: string) =>
    `Anyone who can run commands in the containers of ${namespace} can use your credential while this terminal's container runs.`,
  "warn.restartNeeded": (names: string) => `Takes effect after a restart: ${names}`,
  "warn.reloaderRestarts": (names: string) => `Reloader restarts ${names}`,
  "warn.reloaderMayRestart": (names: string) =>
    `Reloader restarts ${names}, if it runs in this cluster`,
  "warn.tlsHostChanged": (host: string, route: string) =>
    `${route} lists the old host for HTTPS; ${host} needs a new certificate`,
  "warn.gatewayChosen": (gateway: string, of: number) =>
    `Attaches to Gateway ${gateway} (${of} found)`,
  "warn.gatewaysUnreadable": "Unable to read Gateways: Zeabur uses an Ingress",
  "warn.secretValuesUnknown": (name: string) =>
    `Unable to read ${name}'s values: the diff shows keys only`,
  "warn.configNotRolledBack": (workload: string) =>
    `${workload} reads ConfigMaps or Secrets: a rollback leaves their contents as they are`,
  "warn.storageClassUnknown": (claim: string) =>
    `Unable to read the StorageClass of ${claim}: the cluster decides whether it can grow`,
  "warn.readWriteOnceShared": (workload: string) =>
    `${workload} runs several replicas: a ReadWriteOnce volume mounts on one node only`,
  "warn.readWriteOncePodShared": (workload: string) =>
    `${workload} runs several replicas: a ReadWriteOncePod volume serves one replica at a time`,
  "warn.deletesDependents": (owner: string, names: string) =>
    `Also deletes what ${owner} owns: ${names}`,
  "warn.podReplaced": (pod: string, owner: string) =>
    `${owner} starts a new replica in place of ${pod}`,
  "warn.claimDataDeleted": (claim: string, volume: string) =>
    `${claim}'s data is deleted: volume ${volume} has reclaim policy Delete`,
  "warn.claimDataRetained": (claim: string, volume: string) =>
    `Volume ${volume} keeps ${claim}'s data (reclaim policy Retain) until an admin removes it`,
  "warn.claimDataUnknown": (claim: string) =>
    `${claim}'s data may be lost: unable to read its volume's reclaim policy`,
  "warn.stillUsed": (name: string, names: string) =>
    `${names} still read ${name}: their new replicas will not start without it`,
  "warn.claimStillMounted": (name: string, names: string) =>
    `${names} still mount ${name}: Kubernetes removes it once no replica uses it`,
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
    `${name} restarts when a ConfigMap or Secret it reads changes. Nothing restarts now.`,
  "confirm.configReloadOff": (name: string) =>
    `${name} no longer restarts when its ConfigMaps or Secrets change.`,
  "confirm.alsoReads": (names: string, object: string, count: number) =>
    `Also affects ${names} (${count === 1 ? "it reads" : "they read"} ${object}).`,
  "confirm.alsoAffects": (names: string) => `Also affects ${names}.`,
  "confirm.addDomain": (host: string, service: string) => `Serve ${service} at ${host}.`,
  "confirm.renameDomain": (from: string) => `Links to ${from} stop working.`,
  "confirm.removeDomain": (host: string) => `Requests to ${host} no longer reach this project.`,
  "confirm.rollback": (revision: number) =>
    `Template changes made after revision ${revision} are undone.`,
  "confirm.notRolledBack": "A rollback leaves ConfigMap and Secret contents as they are.",
  "confirm.noShrink": "A volume can't shrink after it grows.",
  "confirm.addVolume": (size: string, service: string) => `${service} gets a new ${size} volume.`,
  "confirm.moveMount": (service: string, from: string, to: string) =>
    `Move ${service}'s mount from ${from} to ${to}.`,
  "confirm.readOnly": (path: string, service: string) => `Make ${path} read-only in ${service}.`,
  "confirm.writable": (path: string, service: string) => `Make ${path} writable in ${service}.`,
  "confirm.mount": (service: string) => `Change a mount of ${service}.`,
  "confirm.removeVolume": (service: string, path: string) =>
    `${service} stops mounting ${path}. Its claim and the data on it stay; delete the claim under Objects to free the space.`,
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
  "confirm.isolates": (name: string) =>
    `${name} then accepts and sends only what this and other policies allow; the cluster blocks the rest.`,
  "confirm.delete": (name: string) => `Delete ${name}. This can't be undone.`,
  "action.restart": "Restart",
  "action.saveVariables": "Save variables",
  "action.saveValues": "Save values",
  "action.turnOn": "Turn on",
  "action.turnOff": "Turn off",
  "action.addDomain": "Add domain",
  "action.renameDomain": "Rename domain",
  "action.removeDomain": "Remove domain",
  "action.rollback": (revision: number) => `Roll back to revision ${revision}`,
  "action.grow": (size: string) => `Grow to ${size}`,
  "action.addVolume": "Add volume",
  "action.saveMount": "Save mount",
  "action.removeVolume": "Remove volume",
  "action.saveVolumes": "Save volumes",
  "action.saveHealthChecks": "Save health checks",
  "action.scale": (replicas: number) => `Scale to ${replicas}`,
  "action.scaleMany": (n: number) => `Scale ${n} services`,
  "action.restartMany": (n: number) => `Restart ${n} services`,
  "action.pause": "Pause",
  "action.pauseMany": (n: number) => `Pause ${n} services`,
  "action.saveResources": "Save resources",
  "action.save": "Save",
  "action.saveYaml": "Save YAML",
  "action.isolate": "Isolate",
  "action.allow": "Allow",
  "action.create": "Create",
  "action.delete": "Delete",
  // Toasts
  "toast.region": "Notifications",
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
  "form.cpuUnset": "Not set: nothing reserved. 250m is a quarter of a core.",
  "form.memoryUnset": "Not set: nothing reserved. For example 256Mi.",
  "form.noLimit": "Not set: no limit",
  "form.memoryRequest": "Reserved memory",
  "form.cpuLimit": "Maximum CPU",
  "form.memoryLimit": "Maximum memory",
  "form.computeHint":
    "The node sets the reserved amount aside for each replica. When a replica uses more memory than its maximum, its container restarts; CPU above the maximum is slowed down.",
  "form.largestNode": (cores: string, memory: string) => `Largest node: ${cores} cores, ${memory}`,
  "form.badQuantity": "Use a quantity such as 250m, 1 or 512Mi.",
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
  "form.claimName": "Claim name",
  "form.claimNameHint": "Creates a new claim; the cluster refuses a name already in use.",
  "form.accessMode": "Access mode",
  "form.accessModeHint": "Many-node modes need storage that supports them.",
  "ops.managedEdit": (tool: string) => `Managed by ${tool}: change it at its source`,

  // Editing
  "edit.add": "Add",
  "edit.review": "Review",
  "edit.cancel": "Cancel",
  "edit.edit": (name: string) => `Edit ${name}`,
  "edit.remove": (name: string) => `Remove ${name}`,
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
  "vars.add": "Add variable",
  "vars.name": "Name",
  "vars.value": "Value",
  "vars.container": "Container",
  "data.addKey": "Add key",
  "data.key": "Key",
  "data.readers": "Read by",
  "reload.switch": "Restart when its configuration changes",
  "reload.on": "A change to a ConfigMap or Secret it reads restarts it, whoever makes the change.",
  "reload.off": "It keeps the old values until it is restarted.",
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
  "gap.title": "Missing setup",
  "gap.warning": "Warning",
  "gap.advice": "Advice",
  "gap.unchecked": (count: number) =>
    `${count} ${count === 1 ? "service has" : "services have"} no traffic check`,
  "gap.uncheckedWhy":
    "Without one, Running only means the container started. Each service sets it under Settings.",
  "gap.uncheckedNames": "Show services",
  "gap.fix": "Fix",
  "gap.install": "Install",
  "gap.snippet": "YAML to add",
  "fix.resources": (container: string) => `Set CPU and memory for ${container}`,
  "fix.readiness": (container: string) => `Add a traffic check to ${container}`,
  "fix.addVolume": (container: string, path: string) => `Keep ${path} of ${container} on a disk`,
  "fix.isolateProject": (project: string) => `Isolate the services of ${project}`,
  "gap.unreadable": "Some checks need rights you don't have.",
  "install.title": "Install",
  "install.note": "Zeabur runs none of this. Run it, or commit it to your GitOps repository.",
  "install.profile": "Profile",
  "install.singleNode": "Single node",
  "install.multiNode": "Several nodes",
  "install.forCluster": "fits this cluster",
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
    `On k3s, first disable the packaged ${addon}: add it to disable in /etc/rancher/k3s/config.yaml on every server, then restart k3s.`,
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

  // Usage and history
  "usage.title": "Current usage",
  "usage.history": "History",
  "usage.live": "Live",
  "usage.liveHint":
    "Approximate values from metrics-server, measured about every 15 seconds. Memory is the working set, the figure the kubelet acts on. The tick marks what each replica reserves (its request); the bar ends at its maximum (its limit) and is hatched when there is none.",
  "usage.cores": (n: string) => `${n} cores`,
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
    "Uses more memory than it requests. When the machine runs short, replicas above their request are evicted first.",
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
    `Zeabur looked for ${service}. Any Prometheus-compatible store works, such as Prometheus or VictoriaMetrics; choose its Service in Settings.`,
  "usage.whatHistory": "Seeing usage history",
  "usage.proxyHint":
    "Zeabur reads the metrics store through the Kubernetes API as you. Anyone allowed to do that can read every project's metrics, so usually only administrators have that permission.",
  "usage.storeUnavailable": "The metrics store did not respond.",
  "usage.noSeries": (name: string, range: string) => `No samples for ${name} in the last ${range}.`,
  "usage.noSeriesHint":
    "The store answered but holds nothing for these replicas: it may not collect this machine's container metrics, or not keep data this long.",
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
  "settings.clusterNameChange": "Renaming the cluster",
  "settings.clusterNameLocal":
    "Zeabur runs on this computer and keeps no settings: the name is your kubeconfig's.",
  // Settings: the metrics store
  "settings.metricsStore": "Usage history",
  "settings.store": "History source",
  "settings.storeHint":
    "The Service that holds usage history, as namespace/name:port. Any store with the Prometheus HTTP API works. Zeabur reads it through the Kubernetes API with each person's own rights.",
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
    `Kept in the ConfigMap zeabur-settings in the namespace ${namespace}. Anyone who can change that ConfigMap can change this setting.`,
  "settings.storeDenied": "You can't change it.",
  "settings.storeChange": "Changing the history source",
  "settings.storeLocal":
    "Zeabur runs on this computer and keeps no settings. Start it with --metrics-service namespace/name:port to use another store.",
  "settings.storeTenancy":
    "Anyone who can read the store can read every project's metrics. Grant access only to people who may see the whole cluster.",
  // Account and members (Zeabur Identity)
  "shell.account": "Account",
  "shell.members": "Members",
  "account.title": "Account",
  "account.you": "You",
  "account.username": "User name",
  "account.role": "Role",
  "account.passkeys": "Passkeys",
  "account.backup": "Add a backup passkey on your phone; one lost device must not lock you out.",
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
  "account.means.admin": "Everything a Developer can, and gives others access.",
  "account.means.developer":
    "Everything a Viewer can, and changes services, opens terminals and reads secrets.",
  "account.means.viewer": "Sees services, replicas and logs.",
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
  "linked.none": "None.",
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
    "Inviting and changing members needs rights you don't have here. Ask a cluster administrator.",
  "members.inviting": "Inviting members",
  "members.filter": "Filter members",
  "members.none": "No one can sign in yet.",
  "members.noMatch": "No members match.",
  "members.you": "(you)",
  "members.role.admin": "Admin",
  "members.role.member": "Member",
  "members.adminHint": "Can take on admin rights, which give full control of the cluster.",
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
  "members.notInstalledBody": "Members are the people Zeabur Identity signs in to this cluster.",
  "members.noIssuer":
    "Zeabur doesn't know Zeabur Identity's address, so it can't make setup links. Start Zeabur with --issuer-url.",
  "members.notRedeemed":
    "Zeabur Identity doesn't accept setup links yet: they work once it runs with --invitations.",
  "invite.title": "Invite a member",
  "invite.body":
    "Makes a one-time link that works for 7 days. Whoever opens it first adds a passkey and signs in as this member.",
  "invite.username": "User name",
  "invite.usernameHint": "Lower-case letters, digits, - and . only.",
  "invite.invalid": "Use lower-case letters, digits, - and . only, at most 63.",
  "invite.existing": (user: string) =>
    `${user} is already a member: the link adds a passkey to their account.`,
  "invite.displayName": "Display name",
  "invite.optional": "(optional)",
  "invite.admin": "Admin",
  "invite.create": "Create link",
  "roles.legend": "Access",
  "roles.projects": "Chosen projects",
  "roles.projectsHint":
    "Developer or Viewer in the projects picked below. Every member also sees node and project names.",
  "roles.developer": "Developer",
  "roles.viewer": "Viewer",
  "roles.developerHint":
    "Developers deploy, change variables, restart and open terminals, and can read Secret values.",
  "roles.viewerHint":
    "Viewers see status, logs and events. They can't change anything, open a terminal or read Secret values.",
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
  "roles.losesAdmin": (name: string) => `${name} loses admin rights now.`,
  "roles.losesProjects": (name: string, projects: string) =>
    `${name} loses access to ${projects} now.`,
  "roles.losesChanges": (name: string, projects: string) =>
    `${name} can no longer change ${projects}.`,
  "roles.openTerminals": "A terminal already open stays open until it ends.",
  "roles.becomesAdmin": (name: string) =>
    `${name} gets full control of the cluster: every project, the nodes and the members.`,
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
  "sentence.members.noProjectRoles": () => "Zeabur was installed without the member roles",
  "sentence.members.noProjectRoles.fix": () =>
    "Install the chart with memberRoles.enabled=true, or make the member an admin.",
  "sentence.members.notInstalled": () => "Zeabur Identity isn't installed",
  "sentence.members.notInstalled.fix": () =>
    "Install Zeabur Identity, or start Zeabur with the namespace it runs in.",
  // Account and members end
  // Sentences the server names by key (problems; diagnoses, gaps, traces and incidents are spread in
  // above); `writeSentence` finds them under `sentence.<key>`.
  "sentence.problem.forbidden.fix.askForRule": (p: P) =>
    `Ask a cluster administrator for ${p.rule}, for example with a ClusterRole granting it and a ClusterRoleBinding to you.`,
  "sentence.problem.forbidden.fix.askOrRetry": () => "Ask an admin, or retry as admin.",
  "sentence.problem.forbidden.fix.sendGrant": () =>
    "Send this to an admin. It grants only that permission, and only in this project.",
  "sentence.problem.planExpired.fix.again": (p: P) =>
    `Plans last ${p.minutes} minutes. Make the plan again.`,
  "sentence.problem.badRequest.fix.planToken": () =>
    "Make the plan again. A plan applies only on the server that made it, with its token unchanged.",
  "sentence.problem.conflict.fix.review": () => "Review again to start from its current state.",
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
  "sentence.problem.summary.elevation-required": (p: P) =>
    p.verb
      ? `Your everyday rights can't ${verbEn[p.verb] ?? p.verb} ${thingEn(p)}.`
      : "This needs admin rights.",
  "sentence.problem.summary.not-found": (p: P) =>
    p.name ? `${p.name} doesn't exist.` : "It doesn't exist.",
  "sentence.problem.summary.log-unavailable": () => "This log can't be read.",
  "sentence.problem.summary.conflict": (p: P) =>
    p.name ? `${p.name} changed after you reviewed it.` : "It changed after you reviewed it.",
  "sentence.problem.summary.managed": (p: P) =>
    p.name ? `Another tool manages ${p.name}.` : "Another tool manages this.",
  "sentence.problem.summary.plan-expired": () => "The plan expired.",
  "sentence.problem.summary.system-namespace": (p: P) =>
    `${p.name ?? "This namespace"} belongs to Kubernetes itself.`,
  "sentence.problem.summary.invalid": (p: P) =>
    p.name ? `The cluster refused the change to ${p.name}.` : "The cluster refused the change.",
  "sentence.problem.summary.too-many-requests": () => "The cluster is busy.",
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
  "part.health-checks": "Health checks",
  "form.healthHint":
    "Checks the cluster runs on each replica: whether it takes traffic, whether to restart it, and when it has started.",
  "draft.add": "Add to changes",
  "draft.pendingHere": "has changes waiting for review",
  "draft.count": (n: number) => plural(n, "change pending", "changes pending"),
  "draft.review": "Review",
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
    "Setup the services run without. None of it stops them now; each makes a problem easier to catch or avoid.",
  "project.activity": "Activity",
  "project.next.title": "Deploy the first service",
  "project.next.body":
    "A service shows up here as soon as a workload runs in this namespace: a Deployment, StatefulSet, DaemonSet, Job or CronJob.",
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
    "Causes in the cluster could not all be read; a problem there may be missing.",
  "home.lastKnown": "Last known",
  "home.adoptTitle": "Namespaces to adopt",
  "home.showWarningsOf": (shown: number, total: number) => `Show ${shown} of ${total} warnings`,
  "home.allWarnings": (n: number) => `All ${n} on the cluster page`,
  "home.affects": "Affects",
  "home.inProgress": "In progress",
  "home.problems": "What needs you",
  "shell.context": (cluster: string, user: string) => `Cluster ${cluster}, signed in as ${user}`,
  "home.clusterNotices": (n: number) =>
    n === 1
      ? "1 cluster notice, nothing down because of it:"
      : `${n} cluster notices, nothing down because of them:`,
  "origin.component": "Caused by a cluster component",
  "origin.component.hint":
    "A component the cluster runs for every project causes it, such as the ingress controller or cluster DNS.",
  "origin.zeabur": "Caused by Zeabur",
  "origin.zeabur.hint": "Zeabur itself causes it; report it with the copied diagnosis.",
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
    `${p.object} kept crashing and restarting (${plural(Number(p.count), "time", "times")}).`,
  "sentence.timeline.warning.evicted": (p: P) =>
    `${p.object} was removed from its node (${plural(Number(p.count), "time", "times")})${p.note ? `: ${p.note}` : "."}`,
  "sentence.timeline.warning.unschedulable": (p: P) =>
    `${p.object} found no machine to run on (${plural(Number(p.count), "time", "times")})${p.note ? `: ${p.note}` : "."}`,
  "sentence.timeline.warning.imagePull": (p: P) =>
    `${p.object} couldn't download its image (${plural(Number(p.count), "time", "times")})${p.note ? `: ${p.note}` : "."}`,
  "sentence.timeline.warning.probe": (p: P) =>
    `${p.object} failed a health check (${plural(Number(p.count), "time", "times")})${p.note ? `: ${p.note}` : "."}`,
  "sentence.timeline.warning.quota": (p: P) =>
    `${p.object} couldn't create a replica (${plural(Number(p.count), "time", "times")})${p.note ? `: ${p.note}` : "."}`,
  "sentence.timeline.warning.other": (p: P) =>
    `${p.reason} on ${p.object} (${plural(Number(p.count), "time", "times")})${p.note ? `: ${p.note}` : "."}`,
  "sentence.timeline.termination.oomKilled": (p: P) =>
    `${p.container} in ${p.pod} ran out of memory and was stopped.`,
  "sentence.timeline.termination.exited": (p: P) =>
    `${p.container} in ${p.pod} ended with exit code ${p.exitCode}${p.reason ? ` (${p.reason})` : ""}.`,
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
  "sentence.fix.multiAttach": (p: P) =>
    `The old replica still holds its volume, and a volume attaches to one machine at a time. In YAML, set ${p.service || "the service"}'s strategy.type to Recreate so the old replica stops first; each release then takes it offline for the seconds the new one needs to start.`,
  "sentence.fix.outOfMemory": (p: P) =>
    `Raise the memory maximum${p.limit ? ` above ${p.limit}` : ""}; Usage shows how close it runs. The maximum reserves nothing, so the machine needs that memory free when it is used.`,
  "sentence.fix.evicted.disk":
    "The machine ran short of disk and stopped this replica; a new one starts on its own, without what the old one wrote to its own disk. Keep data on a volume and logs small, or have whoever runs the cluster free disk on that machine.",
  "sentence.fix.evicted.memory":
    "The machine ran short of memory and stopped this replica first. Reserve memory close to what it uses so it goes last; the reservation is held for it even when idle.",
  "sentence.fix.evicted":
    "The machine ran short of resources and stopped this replica; a new one starts on its own. Its events say which resource.",
  "sentence.rollout.stuck.deadlineExceeded": (p: P) =>
    `The rollout of ${p.name} is stuck: it made no progress before its deadline (progressDeadlineSeconds).`,
  "sentence.rollout.stuck.replicaFailure": (p: P) =>
    `The rollout of ${p.name} is stuck: the cluster can't create a new replica.`,
  // ia:start
  "preview.restarts": "What restarts",
  "preview.roll.rolling": (name: string) => `${name} replaces its replicas one at a time.`,
  "preview.roll.recreate": (name: string) => `${name} stops every replica, then starts new ones.`,
  "preview.roll.onDelete": (name: string) =>
    `${name} uses its new settings only when you delete its old replicas.`,
  "preview.scale": (name: string, from: number, to: number) =>
    `${name} goes from ${from} to ${plural(to, "replica", "replicas")}.`,
  "preview.start": (name: string) => `${name} starts.`,
  "preview.stop": (name: string) => `${name} stops.`,
  "preview.pendingRestart": (name: string) => `${name} keeps the old value until it restarts.`,
  "preview.downtime": "Nothing serves for a moment.",
  "preview.cores": (n: string) => `${n} CPU`,
  "preview.fits": (pods: number, cpu: string, memory: string) =>
    `Fits: it adds ${plural(pods, "pod", "pods")}, ${cpu} and ${memory} at its peak.`,
  "preview.blockedQuota": (name: string, resource: string, need: string, left: string) =>
    resource === "pods"
      ? `Quota ${name} allows ${left} more pods; this needs ${need}.`
      : `Quota ${name} allows ${left} more; this needs ${need}.`,
  "preview.blockedNodes": (_name: string, resource: string, need: string, left: string) =>
    resource === "pods"
      ? `No node has room for ${need} more pods; the roomiest has room for ${left}.`
      : `No node has ${need} free; the roomiest has ${left}.`,
  "preview.fitUnknown": "Zeabur could not check whether the new pods fit.",
  "preview.dataLoss": "Data this loses",
  "preview.loss.claimDeleted": (claim: string, paths: string) =>
    `Deletes the disk ${claim}${paths ? ` and what is in ${paths}` : ""}.`,
  "preview.loss.emptyDir": (name: string, volume: string, paths: string) =>
    `${name} keeps ${paths || volume} on a temporary volume; it is emptied when its replicas are replaced.`,
  "preview.loss.statefulSet": (name: string, claim: string) =>
    `${name} deletes the disks it made from ${claim}, with their data.`,
  "preview.removals": "Settings this removes",
  "preview.removal": (path: string, name: string) => `Removes ${path} from ${name}, now`,
  "preview.leftOut": "(left out of your edit)",
  "env.pendingRestart": "May still use the old value until restarted",
  "env.restartByItself": "Restart automatically",
  "env.pendingRestartDetail": (pod: string) =>
    `Changed after ${pod} started. Restart the service to use the new value.`,
  // ia:end
}

export type Messages = typeof en
