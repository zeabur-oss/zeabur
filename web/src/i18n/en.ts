// English: the source of every message key. The other locales are typed as `Messages`, so a key
// missing there (or added there only) is a TypeScript error. Copy is terse: status is one word,
// counts are short, explanations go in tooltips.
import { cuts, probeTiming, type P, type ProbeTiming } from "@/i18n/sentence-parts"

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

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

/** A Secret or ConfigMap in plain words. */
const objectEn = (p: P) => (p.object === "Secret" ? `secret ${p.name}` : `config ${p.name}`)

/** Who a volume's mount failure stops: the container, else the replica. */
const mountedEn = (p: P) => p.container ?? `Replica ${p.pod}`

/** Seconds in words, whole minutes as minutes. */
const secondsEn = (s: number) =>
  s >= 60 && s % 60 === 0
    ? `${s / 60} minute${s === 60 ? "" : "s"}`
    : `${s} second${s === 1 ? "" : "s"}`

/** The sentence, led by "by default" when both values are the defaults, else followed by which one is. */
const timingEn = (t: ProbeTiming, sentence: string) =>
  t.defaults
    ? `By default, ${sentence.charAt(0).toLowerCase()}${sentence.slice(1)}`
    : `${sentence}${t.periodOnly ? ` Every ${secondsEn(t.period)} is the default.` : ""}${
        t.thresholdOnly ? ` ${t.threshold} failures is the default.` : ""
      }`

export const en = {
  // Shell
  "shell.skip": "Skip to content",
  "shell.nav": "Main",
  "shell.home": "Home",
  "shell.projects": "Projects",
  "shell.settings": "Settings",
  "shell.cluster": "Cluster",
  "shell.search": "Search",
  "shell.jumpTo": "Jump to…",
  "shell.menu": "Menu",
  "shell.breadcrumb": "Breadcrumb",
  "shell.findProject": "Find a project",
  "shell.findService": "Find a service",
  "shell.expandSidebar": "Expand sidebar",
  "shell.collapseSidebar": "Collapse sidebar",
  "shell.whoamiUnknown": "Unknown user",
  "shell.account": "Account",
  "shell.signOut": "Sign out",
  // Admin rights: retrying a refused change after a passkey confirmation
  "elevate.retry": "Retry as admin",
  "elevate.title": "Admin rights",
  "elevate.waiting": "Confirm with your passkey in the new window.",
  "elevate.notAllowed": "This Engawa may not ask for admin rights. Ask your cluster administrator.",
  "elevate.blocked": "The browser blocked the window. Allow pop-ups for this site, then try again.",
  "elevate.failed": "Admin rights were not granted.",
  "elevate.signedOut": "Your session ended. Sign in again.",
  "elevate.signIn": "Sign in",
  "elevate.until": (time: string) => `Admin until ${time}`,
  "elevate.activeDetail": "Everything you do uses your admin rights until then.",
  "elevate.drop": "Drop admin rights",
  "elevate.use": "Use admin rights",
  "shell.switchProject": "Switch project",
  "shell.switchService": "Switch service",
  "column.label": "Services",
  "column.other": "Other objects",

  "theme.system": "System theme",
  "theme.light": "Light",
  "theme.dark": "Dark",

  // Common
  "common.loading": "Loading…",
  "common.retry": "Try again",
  "common.all": "All",
  "common.close": "Close",
  "common.copy": "Copy",
  "common.copied": "Copied",
  "common.yes": "yes",
  "common.no": "no",
  "common.status": "Status",
  "common.sep": ", ",
  "common.list": (items: string[]) => new Intl.ListFormat("en").format(items),
  "common.domains": "Domains",
  "common.newTab": "(opens in a new tab)",
  "common.notFound": "This page does not exist.",
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
  "problem.title.internal": "Something went wrong in Engawa",
  "problem.fix.bad-request": "Check the request and try again.",
  "problem.fix.unauthenticated": "Sign in again.",
  "problem.fix.signed-out": "Sign in again.",
  "problem.fix.forbidden": "Ask a cluster administrator for access, or use an account that has it.",
  "problem.fix.elevation-required": "Retry as admin, or ask a cluster administrator for access.",
  "problem.fix.quota-exceeded":
    "Ask for less, free some of what the project uses, or ask a cluster administrator to raise the limit.",
  "problem.fix.policy-denied":
    "Change what the policy names, or ask a cluster administrator about the policy.",
  "problem.fix.not-found": "It may have been deleted or renamed; reload the page.",
  "problem.fix.log-unavailable": "Read the current run's log, or wait a moment and refresh.",
  "problem.fix.conflict": "Reload to see the latest version, then make the change again.",
  "problem.fix.managed":
    "Make the change where the managing tool reads it (Helm values or the Git repository); restarting is still possible here.",
  "problem.fix.plan-expired": "Make the plan again.",
  "problem.fix.system-namespace": "Adopt a namespace of your own instead.",
  "problem.fix.invalid": "Correct the values the cluster names in the details, then retry.",
  "problem.fix.too-many-requests": "Wait a moment and try again.",
  "problem.fix.unavailable":
    "Check that the cluster is running and that this machine can reach it.",
  "problem.fix.internal": "Try again; if it keeps happening, report it with the details.",
  "problem.detail": "Engawa's message",
  "problem.about": "What this kind of problem means",
  // A problem type's page (/problems/{kind}): the rule behind it
  "problemPage.label": "Problem type",
  "problemPage.rule": "The rule",
  "problemPage.kubernetes": "In the Kubernetes documentation",
  "problemPage.rule.bad-request":
    "Engawa checks each request before anything reaches the cluster: what it asks to change must be complete and well formed. A request that is not is refused, and nothing is written.",
  "problemPage.rule.unauthenticated":
    "Every request reaches the cluster with your own sign-in. When the cluster no longer accepts it (it expired, or was revoked), nothing can be read or changed until you sign in again.",
  "problemPage.rule.signed-out":
    "Your Engawa session ended, so Engawa holds no sign-in to act with for you. Nothing is read or changed until you sign in again.",
  "problemPage.rule.forbidden":
    "The cluster decides what each person may do, by rules (RBAC) that say who may read or change which kinds of objects, in which projects. Engawa acts only with your own rights: what they do not allow, it cannot do for you.",
  "problemPage.rule.elevation-required":
    "Your everyday rights do not allow this, but this Engawa can give you admin rights for a short time once you confirm with your passkey. Everything you do then uses them, until they end or you drop them.",
  "problemPage.rule.quota-exceeded":
    "A project can have limits on what all its services may reserve together: CPU, memory, the number of replicas or objects. The cluster refuses a change that would go over them, whatever your rights.",
  "problemPage.rule.policy-denied":
    "A cluster can check every change against policies of its own, run by an admission webhook or a policy engine. When one refuses a change, its own words say why; your rights do not change its answer.",
  "problemPage.rule.not-found":
    "What was asked for is not in the cluster: it may have been deleted or renamed since the page read it.",
  "problemPage.rule.log-unavailable":
    "The cluster keeps a container's log for its current run and the run before, and only while its replica exists. A log never written, or one whose replica is gone, cannot be read.",
  "problemPage.rule.conflict":
    "Engawa writes a change only onto the version you reviewed. When someone or something (a deploy tool, an autoscaler) changed the object after that, nothing is written, so no one's change is lost.",
  "problemPage.rule.managed":
    "Another tool (Helm, Argo CD, Flux) owns this object and puts its own version back at its next sync or upgrade. A change made here would be undone, so Engawa refuses it.",
  "problemPage.rule.plan-expired":
    "A plan is what Engawa will write, as you reviewed it. It holds for a few minutes only, so what is applied still matches the cluster as it is.",
  "problemPage.rule.system-namespace":
    "Kubernetes keeps some namespaces for its own components (kube-system, kube-public, kube-node-lease). They cannot become projects.",
  "problemPage.rule.invalid":
    "The cluster checks each object against its schema and rules before it stores it. It refused this one, and named the fields that are not valid.",
  "problemPage.rule.too-many-requests":
    "The cluster limits how many requests it serves at once, so it stays responsive. It turned this one away for now; nothing was changed.",
  "problemPage.rule.unavailable":
    "Engawa could not reach the cluster's API server: it may be down or restarting, or out of reach from where Engawa runs.",
  "problemPage.rule.internal":
    "Something failed inside Engawa itself, not in the cluster. The details hold what Engawa knows about it.",
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
  "refused.unadopt": (ns: string) => `Letting ${ns} go as a project`,

  // Status words
  "status.ready": "Ready",
  "status.rollingOut": "Rolling out",
  "status.failing": "Failing",
  "status.degraded": "Degraded",
  "status.scheduled": "Scheduled",
  "status.paused": "Paused",
  "status.failed": "Failed",
  "status.scaledToZero": "Scaled to 0",
  "status.completed": "Completed",
  "status.unknown": "Unknown",
  "status.lastExit": (code: number, ago: string) =>
    ago ? `last exit ${code}, ${ago}` : `last exit ${code}`,
  // A failing pod's reason in plain words; the kubelet's own word is in the tooltip
  "reason.crashing": "Keeps crashing",
  "reason.runFailed": "Run failed",
  "reason.outOfMemory": "Out of memory",
  "reason.imagePull": "Can't download image",
  "reason.imageName": "Image name is wrong",
  "reason.configMissing": "Missing setting",
  "reason.cantStart": "Can't start",
  "reason.waitingForRoom": "Waiting for room",
  "reason.starting": "Starting",
  "reason.evicted": "Moved off its machine",
  "reason.releaseStuck": "Release stuck",
  "reason.gaveUp": "Gave up",
  "phase.Running": "Running",
  "phase.Progressing": "Progressing",
  "phase.Degraded": "Degraded",
  "phase.Paused": "Paused",
  "phase.Suspended": "Suspended",
  "phase.Completed": "Completed",
  "phase.Failed": "Failed",
  "phase.Unknown": "Unknown",

  // Roles (service column groups)
  "role.web": "Web",
  "role.internal": "Internal",
  "role.worker": "Workers",
  "role.job": "Jobs",
  "role.data": "Data",

  // Metrics
  "metric.podsReady": "Replicas ready",
  "metric.restarts": "Restarts",
  "metric.warnings": "Warnings",
  "metric.services": "Services",

  "home.title": "Home",
  "home.status": "Status",
  "home.rollingOut": (n: number) => `${n} rolling out`,
  "home.allHealthy": "All healthy",
  "home.healthyIn": (projects: number) =>
    `services healthy in ${plural(projects, "project", "projects")}`,
  "home.nodesReady": (ready: number, total: number) => `${ready}/${total} nodes ready`,
  "home.allProjects": "All projects",
  "home.healthy": "Healthy",
  "home.warningsHour": "Warnings, 1 h",
  "home.noEventsPermission": "You cannot list the cluster's events.",
  "home.servicePods": (name: string, ready: number, desired: number) =>
    `${name}: ${ready}/${desired} ready`,
  "home.noServices": "No services yet",

  // Cluster
  "cluster.title": "Cluster",
  "cluster.tabs": "Cluster pages",
  "cluster.overview": "Overview",
  "cluster.catalogue": "Catalogue",
  "cluster.endOfLife": "End of life",
  "cluster.endOfLifeOn": (date: string) => `End of life ${date}`,
  "cluster.skewBadge": "Unsupported node version",
  "cluster.upgrading": "Upgrading",
  "cluster.available": (version: string) => `${version} available`,
  "cluster.patch": (version: string) => `Patch ${version}`,
  "cluster.nextMinor": (version: string) => `Next minor ${version}`,
  "cluster.allClear": "Nothing needs attention",
  "cluster.more": (n: number) => `${n} more`,
  "cluster.fewer": "Fewer",
  "cluster.attEolSoon": (minor: string, date: string) =>
    `Kubernetes ${minor} reaches end of life on ${date}`,
  "cluster.attEolPast": (minor: string, date: string) =>
    `Kubernetes ${minor} reached end of life on ${date}`,
  "cluster.attEolOld": (minor: string) => `Kubernetes ${minor} is past end of life`,
  "cluster.attSkew": (node: string, version: string) =>
    `${node} runs kubelet ${version}: unsupported skew`,
  "cluster.attTight": (node: string, what: string, percent: string) =>
    `${node}: ${what} requests at ${percent}`,
  "cluster.attNode": (node: string, state: string) => `${node}: ${state}`,
  "cluster.attComponent": (name: string, version: string, tested: string) =>
    `${name} ${version}, tested ${tested}`,
  "cluster.noProjects": "No projects yet.",
  "cluster.nodes": "Nodes",
  "cluster.cpu": "CPU",
  "cluster.memory": "Memory",
  "cluster.cores": (n: string) => `${n} cores`,
  "cluster.requestedOf": (requested: string, allocatable: string) =>
    `${requested} of ${allocatable} requested`,
  "cluster.allocatable": (allocatable: string) => `${allocatable} allocatable`,
  "cluster.used": (used: string) => `${used} used`,
  "cluster.podsOf": (placed: string, room: string) => `${placed} of ${room} pods`,
  "cluster.notReady": "Not ready",
  "cluster.cordoned": "Cordoned",
  "cluster.worker": "worker",
  "cluster.kubelet": "kubelet",
  "cluster.skewMixed": "Another minor than the control plane. Supported.",
  "cluster.skewUnsupported": "Newer than the control plane, or more than 3 minors older.",
  "cluster.noMetrics": "Usage needs metrics-server.",
  "cluster.metricsUnavailable": "metrics-server did not answer.",
  "cluster.noUsageYet": "No usage measured yet.",
  "cluster.partialPods": (n: number) => `Requests from the first ${n} pods only.`,
  "cluster.unscheduled": (n: number) => `${n} pods waiting for a node`,
  "cluster.warnings": "Warnings, last hour",
  "cluster.noWarnings": "No warnings in the last hour.",
  "cluster.moreWarnings": (n: number) => `${n} more`,
  "cluster.partialEvents": "Only the newest 5,000 events were read.",
  "cluster.noComponents": "No catalogue component found.",
  "cluster.tested": (version: string) => `Tested ${version}`,
  "cluster.testedVersion": "Tested version",
  "cluster.newerThanTested": "Newer than tested",
  "cluster.sourceK3s": "k3s",
  "cluster.sourceHelmChart": "HelmChart",
  "cluster.sourceHelm": "Helm",
  "cluster.sourceOther": "—",
  "cluster.roleReadsSecrets":
    "This role can read every Secret in the cluster: passwords, keys and tokens. Grant it only to someone you would trust as a cluster administrator.",
  "cluster.copyRole": "Copy ClusterRole",
  "cluster.whatProjects": "the projects",
  "cluster.whatNodes": "the nodes",
  "cluster.whatRequests": "what pods request",
  "cluster.whatUpgrades": "the upgrade plans",
  "cluster.whatUsage": "node usage",
  "cluster.whatWarnings": "the cluster's warnings",
  "cluster.whatComponents": "the installed components",

  // Projects
  "projects.title": "Projects",
  "projects.adopted": "Project",
  "projects.namespaceOnly": "Namespace",
  "projects.other": "Other namespaces",
  "projects.showSystem": "Show system namespaces",
  "projects.forbiddenTitle": "You cannot list projects",

  // Project
  "project.sections": "Project sections",
  "project.entry.overview": "Overview",
  "project.entry.architecture": "Architecture",
  "project.entry.services": "Services",
  "project.services": "Services",
  "project.events": "Events",
  "project.noServices": "No services in this project.",
  "project.healthyOf": (healthy: number, total: number) => `${healthy}/${total} healthy`,
  "identity.cluster": "Cluster",
  "identity.namespace": "Namespace",

  // Overview
  "attention.title": "Needs attention",
  "attention.none": "All services healthy",
  "attention.ready": (ready: number, desired: number) => `${ready}/${desired} ready`,
  "attention.logsOf": (pod: string) => `Logs of ${pod}`,
  "attention.eventsOf": (pod: string) => `Events of ${pod}`,
  "overview.health": "Project health",
  "overview.updated": "Updated",
  "overview.healthy": "Healthy",
  "overview.failingCount": (n: number) => `${n} failing`,
  "overview.degradedCount": (n: number) => `${n} not ready`,
  "overview.podStrip": "Replicas by service",
  "overview.podsOf": (name: string, ready: number, total: number) =>
    `${name}: ${ready}/${total} ready`,
  "overview.viewAll": "View all",
  "events.show": "Show events",
  "events.warnings": "Warnings",
  "events.noWarnings": "No warnings.",
  "events.empty": "No events.",
  "events.label": "Events, newest first",
  "events.warning": "Warning",
  "events.normal": "Normal",
  "events.forPod": "Replica",
  "events.clearPod": "Show every replica",
  "domains.none": "No domains.",
  "domains.privateOnly": "Cluster only.",
  "domains.https": "HTTPS configured",
  "storage.none": "No volumes.",
  "resource.unused": "Unused",
  "resource.unusedCount": (n: number) => `${n} unused`,
  "storage.requestedTotal": (size: string) => `${size} requested`,
  "storage.notMounted": "Not mounted by any service",

  // Services table
  "table.service": "Service",
  "table.status": "Status",
  "table.ready": "Ready",
  "table.restarts": "Restarts",
  "table.domain": "Domain",
  "table.actions": "Actions",
  "table.kind": "Kind",
  "table.name": "Name",
  "table.pods": "Replicas",
  "services.search": "Filter services",
  "services.attention": "Needs attention",
  "services.healthy": "Healthy",
  "services.allKinds": "All kinds",
  "services.count": (shown: number, total: number) =>
    shown === total ? plural(total, "service", "services") : `${shown} of ${total}`,
  "services.noMatch": "No services match.",

  // Architecture
  "lane.domains": "Domains",
  "lane.web": "Web",
  "lane.internal": "Internal",
  "lane.workers": "Workers & jobs",
  "lane.data": "Data",
  "edge.declared": "Declared",
  "edge.inferred": "Inferred",
  "edge.route": (ingress: string) => `Ingress ${ingress}`,
  "edge.shared": (kind: string, name: string) => `${kind} ${name}`,
  "edge.env": (variable: string) => variable,
  "canvas.zoomIn": "Zoom in",
  "canvas.zoomOut": "Zoom out",
  "canvas.fit": "Fit",
  "canvas.legend": "About the lines",
  "canvas.legendHint":
    "Drawn from Kubernetes objects and variable values, not from traffic. Declared: an Ingress or a shared Secret or ConfigMap. Inferred: a variable's value names the Service.",
  "architecture.edges": (n: number) => plural(n, "connection", "connections"),
  "quickLook.label": (name: string) => `Quick look: ${name}`,
  "quickLook.open": "Open service page",
  "quickLook.upstream": "Upstream",
  "quickLook.downstream": "Downstream",

  // Service page
  "service.tabs": "Service sections",
  "service.more": "More actions",
  "service.delete": "Delete service",
  "service.notFound": "This service does not exist.",
  "service.backTo": (project: string) => `Back to ${project}`,
  "tab.overview": "Overview",
  "tab.logs": "Logs",
  "tab.terminal": "Terminal",
  "tab.variables": "Variables",
  "tab.networking": "Networking",
  "tab.storage": "Storage",
  "tab.events": "Events",
  "ops.restart": "Restart",
  "ops.later": "Arrives in a later version.",
  "cond.available": "Available",
  "cond.allReady": "All replicas ready",
  "cond.stable": "Stable",
  "cond.scheduled": "Schedule",
  "cond.restarting": (n: number) => plural(n, "replica restarted", "replicas restarted"),
  "cond.noRestarts": "No restarts",
  "pod.pods": "Replicas",
  "pod.none": "No replicas.",
  "pod.ready": "Ready",
  "pod.notReady": "Not ready",
  "pod.readyCount": (ready: number, total: number) => `${ready}/${total} ready`,
  "pod.restarts": (n: number) => plural(n, "restart", "restarts"),
  "pod.node": "Node",
  "pod.logsOf": (pod: string) => `Logs of ${pod}`,
  "resources.title": "Resources",
  "resources.hint": "Requests and limits from the pod template. Usage needs metrics-server.",
  "resources.container": "Container",
  "resources.cpu": "CPU",
  "resources.memory": "Memory",
  "resources.requests": "Request",
  "resources.limits": "Limit",
  "connections.title": "Connections",
  "connections.none": "None found.",

  // Logs
  "logs.noPods": "No replicas.",
  "logs.podOf": (pod: string) => `Replica ${pod}`,
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
  "logs.empty": "The log is empty.",
  "logs.noMatch": "No lines match.",
  "logs.jump": "Jump to latest",
  "logs.ended": "Stream ended",
  "logs.refresh": "Refresh",
  "logs.count": (n: number) => plural(n, "line", "lines"),
  "logs.of": (n: number) => `of ${n}`,
  "logs.unavailable.no-previous-run": "No previous run: the container has not restarted.",
  "logs.unavailable.not-started": "No log yet: the container has not started.",
  "logs.unavailable.removed": "The cluster no longer has this run's container, nor its log.",
  "logs.unavailableFix.no-previous-run": "Read the current run's log instead.",
  "logs.unavailableFix.not-started":
    "Wait for it to start; the replica's events say what it is waiting for.",
  "logs.unavailableFix.removed":
    "Wait a moment and refresh. To keep logs past restarts, send them to a log store.",
  "logs.currentRun": "Show current run",
  "terminal.label": "Terminal",
  "terminal.pod": "Replica",
  "terminal.connecting": "Connecting…",
  "terminal.open": "Connected",
  "terminal.reconnect": "Reconnect",
  "terminal.exited": (code: number) => `Exited with code ${code}`,
  "terminal.ended": "Ended",
  "terminal.noShell":
    "This container has no shell. Distroless images ship none; open an ephemeral debug container instead:",
  "terminal.failed": "The container runtime ended the session.",
  "terminal.expired": "Session expired, reconnect.",
  "terminal.adminEnded": "Admin rights ended. Reconnect to go on with your own rights.",
  "terminal.idle": "Closed after 15 minutes without input or output.",
  "terminal.lost": "Connection lost.",
  "terminal.rbac":
    "Opening a terminal needs permission to run commands in this project's containers. Send this to an admin; it grants only that, in this project only:",
  "terminal.roleLabel": "Role and RoleBinding",
  "terminal.debugLabel": "kubectl debug",

  // Variables
  "vars.definedHere": "Defined here",
  "vars.fromObjects": "From other objects",
  "vars.reveal": (secret: string) => `Reveal ${secret}`,
  "vars.hide": (secret: string) => `Hide ${secret}`,
  "vars.secretsHint": "About Secret values",
  "vars.revealHint": "Fetched only when you ask, kept on this page only.",
  "vars.view": "View",
  "vars.list": "List",
  "vars.names": (service: string) => `Names the Service ${service}`,
  "vars.atRuntime": "Set at runtime",
  "vars.allKeys": "All keys",
  "vars.keys": (n: number) => plural(n, "key", "keys"),
  "vars.none": "None.",

  // Networking
  "net.path": "Path",
  "net.via": "Via",
  "net.target": "Target",
  "net.dns": "DNS name",
  "net.ports": "Ports",
  "net.type": "Type",
  "net.anyHost": "Any host",
  "net.defaultBackend": "Default backend",
  "net.private": "Private",
  "net.public": "Public",
  "net.alsoAs": "Also",
  "net.noService":
    "Nothing in the cluster can reach it yet: it has no internal address. Add one so other services or a domain can send it traffic.",

  // Storage
  "storage.noClaims": "No volumes.",
  "storage.requested": "requested",
  "storage.mountPath": "Mount path",
  "storage.accessMode": "Access mode",
  "storage.class": "Storage class",
  "storage.defaultClass": "default",
  "storage.otherMounts": "Other mounts",

  // Palette
  "palette.title": "Command palette",
  "palette.search": "Jump to a project, service, replica or domain",
  "palette.recent": "Recent",
  "palette.actions": "Actions",
  "palette.openLogs": (name: string) => `Open logs of ${name}`,
  "palette.copyKubectl": (name: string) => `Copy kubectl for ${name}`,
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
  "settings.catalogueCommit": "Catalogue",
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
  // A project's namespace: adopting it, letting it go, its details
  "project.entry.settings": "Settings",
  "action.adopt": "Adopt",
  "action.unadopt": "Stop showing as project",
  "action.saveDetails": "Save details",
  "title.adopt": (ns: string) => `Adopt ${ns}`,
  "title.unadopt": (ns: string) => `Stop showing ${ns} as a project`,
  "title.details": (ns: string) => `Details of ${ns}`,
  "confirm.adopt": "Adds the project label to the namespace. Nothing in it changes.",
  "confirm.unadopt": "Nothing in the namespace is deleted. It only stops showing as a project.",
  "confirm.details": "Saves the name, description and icon on the namespace.",
  "access.title": (ns: string) => `You cannot change ${ns}`,
  "access.you": "<your user name>",
  "access.ask": (ns: string) => `Ask a cluster admin for patch on the Namespace ${ns}.`,
  "access.alsoLabels":
    "This right also lets you change the namespace's other labels, including its Pod Security levels (pod-security.kubernetes.io/*). Kubernetes cannot limit it to one label.",
  "access.role": "A ClusterRole limited to this namespace with resourceNames:",
  "access.roleLabel": "ClusterRole and binding",
  "access.kubectl": "Or an admin can run:",
  "access.kubectlLabel": "kubectl command",
  "projectSettings.details": "Details",
  "projectSettings.displayName": "Display name",
  "projectSettings.description": "Description",
  "projectSettings.icon": "Icon",
  "projectSettings.tooLong": (max: number) => `At most ${max} characters`,
  "projectSettings.readOnly": "You can see these but not change them.",
  "projectSettings.getAccess": "How to get access",
  "projectSettings.changeDetails": "Changing this project's details",
  "projectSettings.project": "Project",
  "projectSettings.adopted": (ns: string) => `${ns} shows as a project.`,
  "projectSettings.notAdopted": (ns: string) => `${ns} is a namespace, not a project.`,
  "projectSettings.system":
    "Kubernetes keeps this namespace for its own components; it cannot be a project.",
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
  "objects.none": "No objects.",
  "objects.owner": (kind: string) => `by ${kind}`,
  "objects.empty": "Empty",
  "objects.scan": "Scan all kinds",
  "objects.scanning": "Scanning…",
  "objects.scanned": "All kinds read",
  "objects.updated": "updated",
  "objects.refresh": "Refresh",
  "objects.load": (kind: string) => `Read ${kind}`,
  "objects.unread": "not read",
  "objects.denied": "no access",
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
  "plan.diff": "YAML diff",
  "plan.create": "New",
  "plan.delete": "Deleted",
  "plan.typeName": (name: string) => `Type ${name} to confirm`,
  "plan.added": "added",
  "plan.removed": "removed",
  "warn.restartNeeded": (names: string) => `Takes effect after a restart: ${names}`,
  "warn.restartAdded": (names: string) => `Also restarts ${names}`,
  "warn.tlsHostChanged": (host: string, route: string) =>
    `${route} lists the old host for HTTPS; ${host} needs a new certificate`,
  "warn.gatewayChosen": (gateway: string, of: number) =>
    `Attaches to Gateway ${gateway} (${of} found)`,
  "warn.gatewaysUnreadable": "Gateways not readable: uses an Ingress",
  "warn.secretValuesUnknown": (name: string) =>
    `${name}'s values not readable: diff shows keys only`,
  "warn.configNotRolledBack": (workload: string) =>
    `${workload} reads ConfigMaps or Secrets: their contents are not rolled back`,
  "warn.storageClassUnknown": (claim: string) =>
    `StorageClass of ${claim} not readable: the cluster decides whether it can grow`,
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
    `${claim}'s data may be lost: its volume's reclaim policy is not readable`,
  "warn.stillUsed": (name: string, names: string) =>
    `${names} still read ${name}: their new replicas will not start without it`,
  "warn.claimStillMounted": (name: string, names: string) =>
    `${names} still mount ${name}: it goes once no replica uses it`,
  "warn.ingressOpened": (names: string) =>
    `No longer isolated: ${names} accept traffic from anywhere`,
  "warn.egressOpened": (names: string) => `No longer limited: ${names} may send anywhere`,
  "warn.connectionsOpened": (pairs: string) => `Allowed again: ${pairs}`,
  // Confirmations: what will happen, in plain words
  "confirm.restartSingle": (name: string) =>
    `${name} has one replica, so it may be down for a few seconds while it restarts.`,
  "confirm.restartNone": (name: string) =>
    `No replica of ${name} is ready now; restarting replaces them all.`,
  "confirm.rolling": (name: string, pods: number) =>
    `${name}'s ${pods} replicas are replaced one at a time, so the service stays up.`,
  "confirm.rollingSingle": (name: string) =>
    `${name} has one replica, so it may be down for a few seconds while it is replaced.`,
  "confirm.rollingNone": (name: string) => `${name}'s replicas are replaced.`,
  "confirm.envSet": (names: string, service: string) => `Set ${names} on ${service}.`,
  "confirm.envRemove": (names: string, service: string) => `Remove ${names} from ${service}.`,
  "confirm.envBoth": (set: string, removed: string, service: string) =>
    `Set ${set} and remove ${removed} on ${service}.`,
  "confirm.dataSet": (keys: string, kind: string, name: string) =>
    `Set ${keys} in ${kind} ${name}.`,
  "confirm.dataRemove": (keys: string, kind: string, name: string) =>
    `Delete ${keys} from ${kind} ${name}. The old value cannot be brought back here.`,
  "confirm.dataBoth": (set: string, removed: string, kind: string, name: string) =>
    `Set ${set} and delete ${removed} in ${kind} ${name}. A deleted value cannot be brought back here.`,
  "confirm.keepsOld": (names: string, count: number) =>
    `${names} ${count === 1 ? "keeps" : "keep"} the old value until restarted.`,
  "confirm.readersKeepOld": (names: string, object: string, count: number) =>
    `${names} ${count === 1 ? "reads" : "read"} ${object} and ${count === 1 ? "keeps" : "keep"} the old value until restarted.`,
  "confirm.alsoReads": (names: string, object: string, count: number) =>
    `Also affects ${names} (${count === 1 ? "it reads" : "they read"} ${object}).`,
  "confirm.alsoAffects": (names: string) => `Also affects ${names}.`,
  "confirm.addDomain": (host: string, service: string) => `Serve ${service} at ${host}.`,
  "confirm.renameDomain": (from: string) => `Links to ${from} stop working.`,
  "confirm.removeDomain": (host: string) => `Requests to ${host} no longer reach this project.`,
  "confirm.rollback": (revision: number) =>
    `Template changes made after revision ${revision} are undone.`,
  "confirm.notRolledBack": "ConfigMap and Secret contents are not rolled back.",
  "confirm.noShrink": "Once grown, the volume cannot shrink again.",
  "confirm.addVolume": (size: string, service: string) => `${service} gets a new ${size} volume.`,
  "confirm.moveMount": (service: string, from: string, to: string) =>
    `Move ${service}'s mount from ${from} to ${to}.`,
  "confirm.readOnly": (path: string, service: string) => `Make ${path} read-only in ${service}.`,
  "confirm.writable": (path: string, service: string) => `Make ${path} writable in ${service}.`,
  "confirm.mount": (service: string) => `Change a mount of ${service}.`,
  "confirm.probeSet.readiness": (check: string, period: number, _failures: number) =>
    `A replica gets no traffic until ${check} succeeds (checked every ${period} s).`,
  "confirm.probeSet.liveness": (check: string, period: number, failures: number) =>
    `If ${check} fails ${failures} times in a row, the container is restarted (checked every ${period} s).`,
  "confirm.probeSet.startup": (check: string, period: number, failures: number) =>
    `Other checks wait until ${check} succeeds; after ${failures} failures the container is restarted (checked every ${period} s).`,
  "confirm.probeRemoved.readiness": "Without it, a replica gets traffic as soon as it starts.",
  "confirm.probeRemoved.liveness": "Without it, a container that hangs is not restarted.",
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
    "Your YAML is written as it is: every field you changed takes effect at once.",
  "confirm.create": (kind: string, name: string) => `Create ${kind} ${name}.`,
  "confirm.isolates": (name: string) =>
    `${name} then accepts and sends only what this and other policies allow; the rest is blocked.`,
  "confirm.delete": (name: string) => `Delete ${name}. This cannot be undone.`,
  "action.restart": "Restart",
  "action.saveVariables": "Save variables",
  "action.saveValues": "Save values",
  "action.saveAndRestart": "Save and restart",
  "action.addDomain": "Add domain",
  "action.renameDomain": "Rename domain",
  "action.removeDomain": "Remove domain",
  "action.rollback": (revision: number) => `Roll back to revision ${revision}`,
  "action.grow": (size: string) => `Grow to ${size}`,
  "action.addVolume": "Add volume",
  "action.saveMount": "Save mount",
  "action.saveHealthChecks": "Save health checks",
  "action.scale": (replicas: number) => `Scale to ${replicas}`,
  "action.saveResources": "Save resources",
  "action.save": "Save",
  "action.saveYaml": "Save YAML",
  "action.isolate": "Isolate",
  "action.allow": "Allow",
  "action.create": "Create",
  "action.delete": "Delete",
  // Toasts: the outcome of an apply
  "toast.region": "Notifications",
  "toast.applied": "Applied",
  "toast.view": "View",
  "toast.undo": "Undo",
  "toast.undone": "Undone",
  "toast.managed": (tool: string) => `Managed by ${tool}`,
  "toast.managedHelm": "Change it in the chart's values and upgrade the release.",
  "toast.managedGit": (tool: string) => `Change it in the Git repository ${tool} syncs from.`,
  "title.env": (service: string) => `Variables of ${service}`,
  "title.data": (kind: string, name: string) => `${kind} ${name}`,
  "title.restart": (service: string) => `Restart ${service}`,
  "title.addDomain": (host: string) => `Add ${host}`,
  "title.renameDomain": (from: string, to: string) => `${from} → ${to}`,
  "title.removeDomain": (host: string) => `Remove ${host}`,
  "tab.settings": "Settings",
  "tab.releases": "Releases",
  "yaml.edit": "Edit",
  "yaml.editSecret": "Values left as (hidden) are kept.",
  "yaml.notEditable": "Kubernetes manages this kind itself; edit the object that owns it.",
  "title.editYaml": (name: string) => `Edit ${name}`,
  "title.delete": (kind: string, name: string) => `Delete ${kind} ${name}`,
  "yaml.delete": "Delete",
  "yaml.notDeletable": "Kubernetes makes this kind itself; delete the object that owns it.",
  "title.rollback": (service: string, revision: number) => `Roll back ${service} to #${revision}`,
  "title.compute": (service: string) => `Compute of ${service}`,
  "title.probe": (service: string) => `Health checks of ${service}`,
  "title.expand": (claim: string, size: string) => `Grow ${claim} to ${size}`,
  "title.mount": (path: string) => `Change mount ${path}`,
  "title.addVolume": (path: string) => `Add a volume at ${path}`,
  "releases.none": "No releases: Jobs and CronJobs keep none.",
  "releases.current": "Running",
  "releases.compare": "Compare",
  "releases.rollback": "Roll back",
  "releases.diff": (from: number, to: number) => `#${from} → #${to}`,
  "releases.same": "The two templates are the same.",
  "form.compute": "Compute",
  "form.health": "Health checks",
  "form.jobs": "Jobs run once; change their template as YAML.",
  "form.replicas": "Replicas",
  "form.cpuRequest": "CPU request",
  "form.memoryRequest": "Memory request",
  "form.cpuLimit": "CPU limit",
  "form.memoryLimit": "Memory limit",
  "form.largestNode": (cores: string, memory: string) => `Largest node: ${cores} cores, ${memory}`,
  "form.badQuantity": "Use a quantity such as 250m, 1 or 512Mi.",
  "form.badReplicas": "A whole number, 0 to 1000.",
  "form.badNumber": "A whole number above 0.",
  "form.badPath": "An absolute path, such as /data.",
  "form.badName": "Lower-case letters, digits and -.",
  "form.probe.readiness": "Traffic check",
  "form.probe.liveness": "Restart check",
  "form.probe.startup": "Startup check",
  "form.probeEffect.readiness": "Traffic goes only to replicas that pass.",
  "form.probeEffect.liveness": "A container that keeps failing is restarted.",
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
  "form.size": "Size",
  "form.sizeHint": "Claims grow, never shrink.",
  "form.expand": "Grow",
  "form.readOnly": "Read-only",
  "form.addVolume": "Add a volume",
  "form.volumeName": "Name",
  "form.statefulVolumes": "A StatefulSet's volumes are set when it is created.",
  "form.advanced": "Advanced",
  "form.claimName": "Claim name",
  "form.claimNameHint": "A new claim; a name already in use is refused.",
  "form.badClaimName": "Lower-case letters, digits, - and ., at most 253.",
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
  "vars.add": "Add variable",
  "vars.name": "Name",
  "vars.value": "Value",
  "vars.container": "Container",
  "vars.editIn": (kind: string, name: string) => `Edit in ${kind} ${name}`,
  "data.addKey": "Add key",
  "data.key": "Key",
  "data.restartAfter": "Restart readers after apply",
  "data.readers": "Read by",
  "data.reveal": "Reveal values",
  "data.binary": "binary",
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
  "gap.title": "Missing",
  "gap.warning": "Warning",
  "gap.advice": "Advice",
  "gap.fix": "Fix",
  "gap.install": "Install",
  "gap.snippet": "YAML to add",
  "fix.resources": (container: string) => `Set CPU and memory for ${container}`,
  "fix.readiness": (container: string) => `Add a traffic check to ${container}`,
  "fix.isolateProject": (project: string) => `Isolate the services of ${project}`,
  "gap.unreadable": "Some checks need rights you don't have.",
  "install.title": "Install",
  "install.note": "Engawa runs none of this. Run it, or commit it to your GitOps repository.",
  "install.profile": "Profile",
  "install.singleNode": "Single node",
  "install.multiNode": "Several nodes",
  "install.forCluster": "fits this cluster",
  "install.needs": "Needs first",
  "install.security": "Access",
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
  "reach.title": "Reachability",
  "reach.addPolicy": "Add policy",
  "reach.isolate": "Isolate this service",
  "reach.allowIngress": "Allow the ingress controller",
  "reach.allowFrom": (name: string) => `Allow from ${name}`,
  "reach.planIsolate": (name: string) => `Isolate ${name}`,
  "reach.planAllowIngress": (name: string) => `Let the ingress controller reach ${name}`,
  "reach.planAllowFrom": (from: string, to: string) => `Let ${from} reach ${to}`,
  "reach.policies": "NetworkPolicy",
  "reach.noPolicies": "None in this project: everything may connect.",
  "reach.noneSelect": "None select this service.",
  "reach.in": "In",
  "reach.out": "Out",
  "reach.ingressController": "Ingress controller",
  "reach.otherNamespaces": "Other namespaces",
  "reach.dns": "DNS",
  "reach.outside": "Outside the cluster",
  "reach.cni": "Whether these rules are enforced depends on the cluster's network plugin (CNI).",
  "reach.allowed": "Allowed",
  "reach.limited": "Partly",
  "reach.blocked": "Blocked",
  "reach.edgeBlocked": "Blocked by a NetworkPolicy",
  "reach.edgeLimited": "Only partly allowed by NetworkPolicy",
  "backups.title": "Backups",
  "backups.namespace": "Objects of the namespace",
  "backups.unscheduled": "No schedule",
  "backups.none": "Not backed up",
  "backups.noRun": "No backup yet",
  "backups.noProvider": "No backup component is installed.",
  "backups.seeComponents": "Components",
  "backups.refused": (module: string) => `You may not read ${module}'s backups.`,
  "backups.details": "Schedules and runs",
  "backups.paused": "Paused",
  "backups.succeeded": "Succeeded",
  "backups.partiallyFailed": "Partly failed",
  "backups.failed": "Failed",
  "backups.running": "Running",
  "backups.unknown": "Unknown",
  "backups.veleroVolumesNeedSnapshots": "volume data only with snapshots",
  "backups.scheduledOf": (scheduled: number, total: number) => `${scheduled}/${total} scheduled`,

  // Usage and history (requests are "reserved", limits the "maximum"; the Kubernetes words are
  // in the hints)
  "usage.title": "Usage",
  "usage.history": "History",
  "usage.live": "Live",
  "usage.liveHint":
    "From metrics-server, measured every 15 s or so, approximate. Memory is the working set, the figure the kubelet acts on. The tick is what each replica reserves (its request); the end of a bar is its maximum (its limit), hatched when it has none.",
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
    `${pod} is close to its memory maximum and may be restarted.`,
  "usage.nearLimitMany": (n: number) =>
    `${n} replicas are close to their memory maximum and may be restarted.`,
  "usage.needsMetricsServer": "Live usage needs metrics-server.",
  "usage.install": "Install",
  "usage.whatLive": "Seeing live usage",
  "usage.unavailable": "metrics-server did not answer. Reserved and maximum amounts are shown.",
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
    `Engawa looked for ${service}. Any Prometheus-compatible store works (Prometheus, VictoriaMetrics); its Service is set in Settings.`,
  "usage.whatHistory": "Seeing usage history",
  "usage.proxyHint":
    "Engawa reads the metrics store through the Kubernetes API as you. Whoever may do so can read every project's metrics, so it is usually kept to administrators.",
  "usage.storeUnavailable": "The metrics store did not answer.",
  "usage.noSeries": (name: string, range: string) => `No samples for ${name} in the last ${range}.`,
  "usage.noSeriesHint":
    "The store answered but holds nothing for these replicas: it may not collect this machine's container metrics, or not keep data this long.",
  "usage.source": "From",
  "usage.stepSeconds": (n: number) => `a point every ${n} s`,
  "usage.stepMinutes": (n: number) => `a point every ${n} min`,
  "usage.tenancy": "Who can see this",
  "usage.tenancyHint":
    "Engawa asks only for this project's replicas. The store itself is not split by project: anyone allowed to open it can read every project's metrics.",
  "usage.loadingChart": "Loading the chart",

  // Settings: the metrics store
  "settings.metricsStore": "Usage history",
  "settings.store": "History source",
  "settings.storeHint":
    "The Service history is read from, as namespace/name:port. Any store with the Prometheus HTTP API works. Engawa reads it through the Kubernetes API as each person.",
  "settings.storeDefault": "Default",
  "settings.storeChosen": "Chosen in Engawa",
  "settings.storeState": "Status",
  "settings.storeState.available": "Answers",
  "settings.storeState.not-configured": "Not found",
  "settings.storeState.unreadable": "Not allowed for you",
  "settings.storeState.unavailable": "Not answering",
  "settings.storeSave": "Save",
  "settings.storeReset": "Use the default",
  "settings.storeSaved": "Saved",
  "settings.storeWho": "Usually only cluster administrators can change this.",
  "settings.storeWhereLabel": "Where this is kept",
  "settings.storeWhere": (namespace: string) =>
    `Kept in the ConfigMap engawa-settings in namespace ${namespace}: whoever may change that ConfigMap may change this.`,
  "settings.storeDenied": "You may not change it.",
  "settings.storeChange": "Changing the history source",
  "settings.storeLocal":
    "Engawa runs on this computer and keeps no settings. Start it with --metrics-service namespace/name:port to use another store.",
  "settings.storeTenancy":
    "Whoever may read the store can read every project's metrics. Grant it only to people who may see the whole cluster.",
  // Sentences the server names by key (diagnoses, gaps, problems), with their values: the
  // server never sends prose. `writeSentence` finds them under `sentence.<key>`.
  "sentence.problem.forbidden.fix.askForRule": (p: P) =>
    `Ask a cluster administrator for ${p.rule}, for example with a ClusterRole granting it and a ClusterRoleBinding to you.`,
  "sentence.problem.forbidden.fix.askOrRetry": () => "Ask an admin, or retry as admin.",
  "sentence.problem.forbidden.fix.sendGrant": () =>
    "Send this to an admin; it grants only that, in this project only.",
  "sentence.problem.planExpired.fix.again": (p: P) =>
    `Plans last ${p.minutes} minutes. Make the plan again.`,
  "sentence.problem.badRequest.fix.planToken": () =>
    "Make the plan again; a plan applies only with the token it came with, unchanged, on the server that made it.",
  "sentence.problem.conflict.fix.review": () => "Review again to start from its current state.",
  "sentence.problem.conflict.fix.waitForContainer": () =>
    "Wait until it runs; its logs and events say why it doesn't.",
  "sentence.problem.unavailable.fix.tryAgainSoon": () => "Try again in a moment.",
  "sentence.problem.logUnavailable.fix.noPreviousRun": () => "Read the current run's log instead.",
  "sentence.problem.logUnavailable.fix.notStarted": () =>
    "Wait for the container to start; the replica's events say what it is waiting for.",
  "sentence.problem.logUnavailable.fix.removed": () =>
    "Wait a moment and refresh; if it stays unavailable, read the current run's log. To keep logs past restarts, send them to a log store.",
  "sentence.problem.quotaExceeded.fix.scale": (p: P) =>
    p.object
      ? `Scale ${p.object} to fewer, lower what it reserves, or ask an admin to raise the limit ${p.quota}.`
      : `Ask for less, free some of what the project uses, or ask an admin to raise the limit ${p.quota}.`,
  "sentence.problem.policyDenied.fix.change": () =>
    "Change what it names, or ask whoever runs the policy.",
  "sentence.diagnosis.crashLoop.why.backOff": (p: P) =>
    `It has restarted ${p.restarts} times; the cluster starts it again each time, waiting longer between tries, up to 5 minutes by default.`,
  "sentence.diagnosis.crashLoop.why.exitsZero": (p: P) =>
    `A service is meant to keep running, so the cluster starts it again whenever its program ends, even cleanly (${p.restarts} times so far).`,
  "sentence.diagnosis.runFailed.why.exitCode": (p: P) =>
    `A run counts as failed when its program ends with an exit code other than 0; this one ended with ${p.exitCode}.`,
  "sentence.diagnosis.runFailed.why.gaveUp": (p: P) =>
    p.backoffLimit
      ? `The task gave up: it retries a failed run ${p.backoffLimit} times, and the last one ended with ${p.exitCode} too.`
      : `The task gave up after its retries; the last run ended with ${p.exitCode}.`,
  "sentence.diagnosis.outOfMemory.why.limit": (p: P) =>
    `Each replica may use at most ${p.limit} of memory; at that point the system stops it at once, and the program can't catch it.`,
  "sentence.diagnosis.outOfMemory.why.noLimit": () =>
    "With no memory maximum, it can use the machine's memory until the machine runs short; then the system stops what uses the most.",
  "sentence.diagnosis.imagePull.why": () =>
    "The cluster downloads the image before it starts the container, and keeps retrying, waiting longer each time, until the download works.",
  "sentence.diagnosis.probeFailure.why.liveness": (p: P) => {
    const t = probeTiming(p)
    return t
      ? timingEn(
          t,
          `The cluster checks every ${secondsEn(t.period)} and restarts the container after ${t.threshold} failures in a row (about ${secondsEn(t.total)}).`,
        )
      : "When the restart check fails several times in a row, the cluster restarts the container."
  },
  "sentence.diagnosis.probeFailure.why.readiness": (p: P) => {
    const t = probeTiming(p)
    return t
      ? timingEn(
          t,
          `The cluster checks every ${secondsEn(t.period)}; after ${t.threshold} failures in a row (about ${secondsEn(t.total)}) the container gets no traffic from its internal addresses until it passes again.`,
        )
      : "While the traffic check fails, the container gets no traffic from its internal addresses."
  },
  "sentence.diagnosis.probeFailure.why.startup": (p: P) => {
    const t = probeTiming(p)
    return t
      ? timingEn(
          t,
          `The container has ${t.threshold} × ${secondsEn(t.period)} (${secondsEn(t.total)}) to pass the startup check; then the cluster restarts it.`,
        )
      : "The other checks wait for the startup check; if it doesn't pass in time, the cluster restarts the container."
  },
  "sentence.diagnosis.configMissing.cause.object": (p: P) =>
    `${p.container} can't start: ${objectEn(p)} doesn't exist.`,
  "sentence.diagnosis.configMissing.cause.key": (p: P) =>
    `${p.container} can't start: ${objectEn(p)} has no ${p.key}.`,
  "sentence.diagnosis.configMissing.cause.unknown": (p: P) =>
    `${p.container} can't start: a secret or config it reads is missing.`,
  "sentence.diagnosis.configMissing.cause.volume": (p: P) =>
    `${mountedEn(p)} can't start: volume ${p.volume} comes from ${objectEn(p)}, which doesn't exist.`,
  "sentence.diagnosis.configMissing.why": () =>
    "The cluster starts a container only once every secret and config it reads, and every key it names, exists.",
  "sentence.diagnosis.configMissing.why.volume": () =>
    "The cluster mounts every volume before it starts a replica's containers; a volume from a secret or config that doesn't exist can't be mounted, so they keep waiting.",
  "sentence.diagnosis.configMissing.fix.object": (p: P) =>
    `Add ${objectEn(p)} to the project, or stop reading it.`,
  "sentence.diagnosis.configMissing.fix.key": (p: P) =>
    `Add ${p.key} to ${objectEn(p)}, or stop reading it.`,
  "sentence.diagnosis.configMissing.fix.unknown": () =>
    "The replica's events name what is missing; add it, or stop reading it.",
  "sentence.diagnosis.configMissing.fix.volume": (p: P) =>
    `Add ${objectEn(p)} to the project, or remove volume ${p.volume}.`,
  "sentence.diagnosis.unschedulable.cause.cpu": (p: P) =>
    `${p.service} is waiting for room: no machine has enough CPU free.`,
  "sentence.diagnosis.unschedulable.cause.memory": (p: P) =>
    `${p.service} is waiting for room: no machine has enough memory free.`,
  "sentence.diagnosis.unschedulable.cause.placement": (p: P) =>
    `${p.service} is waiting for a machine it is allowed to run on.`,
  "sentence.diagnosis.unschedulable.cause.other": (p: P) =>
    `${p.service} is waiting for a machine to run on.`,
  "sentence.diagnosis.unschedulable.why": (p: P) =>
    `Each replica goes on a machine with room for what it reserves; the cluster says: ${p.message}`,
  "sentence.diagnosis.unschedulable.fix.cpu": () =>
    "Reserve less CPU, run fewer replicas, or add a machine.",
  "sentence.diagnosis.unschedulable.fix.memory": () =>
    "Reserve less memory, run fewer replicas, or add a machine.",
  "sentence.diagnosis.unschedulable.fix.placement": () =>
    "Check where it is allowed to run (node selector, affinity, tolerations).",
  "sentence.diagnosis.unschedulable.fix.other": () =>
    "The replica's events have the scheduler's full answer.",
  "sentence.diagnosis.quotaBlocked.cause": (p: P) =>
    `${p.service} can't start more replicas: the project's limit ${p.quota} is reached.`,
  "sentence.diagnosis.quotaBlocked.why": (p: P) =>
    `The project limit ${p.quota} caps what all its services may reserve together; the cluster refuses new replicas over it.`,
  "sentence.diagnosis.quotaBlocked.fix": () =>
    "Run fewer replicas, reserve less per replica, or ask an admin to raise the limit.",
  "sentence.diagnosis.unreachable.cause.noService": (p: P) =>
    `Nothing in the cluster can reach ${p.service} yet: it has no internal address.`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p: P) =>
    `${p.endpoint} sends to port ${p.targetPort}, which ${p.service}'s containers don't declare${p.ports ? ` (they declare ${p.ports})` : ""}.`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p: P) =>
    `${p.endpoint} has nowhere to send traffic: no replica of ${p.service} is ready.`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p: P) =>
    `Requests to the domain of ${p.route} don't reach ${p.service}: its entry point reports ${p.condition} ${p.reason}.`,
  "sentence.diagnosis.unreachable.why.noService": (p: P) =>
    `${p.from} sends traffic to ${p.service} by name, but only an internal address with that name can receive it.`,
  "sentence.diagnosis.unreachable.why.targetPort": () =>
    "An internal address forwards to the port it names; a named port must be one the containers declare.",
  "sentence.diagnosis.unreachable.why.noReadyEndpoints": () =>
    "An internal address sends traffic only to replicas that pass their traffic check.",
  "sentence.diagnosis.unreachable.why.routeNotAccepted": () =>
    "A domain's entry point must accept the route and find the address it names before requests go through.",
  "sentence.diagnosis.unreachable.fix.noService": () =>
    "Add an internal address so other services or a domain can send it traffic.",
  "sentence.diagnosis.unreachable.fix.targetPort": () =>
    "Point the internal address at a port the containers declare.",
  "sentence.diagnosis.unreachable.fix.noReadyEndpoints": () =>
    "Find out why no replica is ready: its other problems, its traffic check, its logs.",
  "sentence.diagnosis.unreachable.fix.routeNotAccepted": () =>
    "Check the route's entry point and the address it sends to; the route's status says which.",
  "sentence.diagnosis.unexplained.cause": (p: P) =>
    p.reason ? `${p.service} is failing (${p.reason}).` : `${p.service} is failing.`,
  "sentence.diagnosis.unexplained.why": (p: P) =>
    p.reason
      ? `Engawa has no explanation for the state the cluster reports, ${p.reason}.`
      : "Engawa has no explanation for the state the cluster reports.",
  "sentence.diagnosis.unexplained.fix": () => "Its events and logs may say more.",
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
  "sentence.problem.summary.internal": () => "Something went wrong in Engawa.",
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
  "sentence.diagnosis.crashLoop.cause.exitCode": (p: P) =>
    `${p.container} keeps crashing and restarting.`,
  "sentence.diagnosis.crashLoop.cause.exitsZero": (p: P) =>
    `${p.container} keeps ending without an error, and restarting.`,
  "sentence.diagnosis.crashLoop.cause.notExecutable": (p: P) =>
    `${p.container} can't run its command: the file isn't executable.`,
  "sentence.diagnosis.crashLoop.cause.commandNotFound": (p: P) =>
    `${p.container} can't run its command: it isn't in the image.`,
  "sentence.diagnosis.crashLoop.cause.killed": (p: P) =>
    `${p.container} keeps being killed, and restarting.`,
  "sentence.diagnosis.crashLoop.cause.terminated": (p: P) =>
    `${p.container} keeps being stopped, and restarting.`,
  "sentence.diagnosis.crashLoop.fix.exitsZero": () =>
    "A service must keep running; run one-off work as a task (Job).",
  "sentence.diagnosis.crashLoop.fix.command": () =>
    "Check the image's entrypoint, command and args.",
  "sentence.diagnosis.crashLoop.fix.killed": () =>
    "Something killed it: check its memory use and its restart check.",
  "sentence.diagnosis.crashLoop.fix.terminated": () =>
    "Check its restart check, and what stops the program.",
  "sentence.diagnosis.crashLoop.fix.logs": () =>
    "The error is in the logs of the run before the restart.",
  "sentence.diagnosis.runFailed.cause.exitCode": (p: P) => `${p.container}'s last run failed.`,
  "sentence.diagnosis.runFailed.cause.exitsZero": (p: P) =>
    `${p.container}'s last run ended unexpectedly.`,
  "sentence.diagnosis.runFailed.cause.notExecutable": (p: P) =>
    `${p.container}'s last run couldn't start its command: the file isn't executable.`,
  "sentence.diagnosis.runFailed.cause.commandNotFound": (p: P) =>
    `${p.container}'s last run couldn't find its command in the image.`,
  "sentence.diagnosis.runFailed.cause.killed": (p: P) => `${p.container}'s last run was killed.`,
  "sentence.diagnosis.runFailed.cause.terminated": (p: P) =>
    `${p.container}'s last run was stopped.`,
  "sentence.diagnosis.outOfMemory.cause.limit": (p: P) =>
    `${p.container} ran out of memory and was stopped.`,
  "sentence.diagnosis.outOfMemory.cause.noLimit": (p: P) =>
    `${p.container} was stopped because the machine ran short of memory.`,
  "sentence.diagnosis.outOfMemory.fix.limit": () =>
    "Raise the memory maximum, or find what uses the memory.",
  "sentence.diagnosis.outOfMemory.fix.noLimit": () =>
    "Set how much memory it reserves, and its maximum, to fit what it uses.",
  "sentence.diagnosis.imagePull.cause.notFound": (p: P) =>
    `${p.container} can't download its image ${p.image ?? ""}: the registry has no such image.`,
  "sentence.diagnosis.imagePull.cause.notFoundOrPrivate": (p: P) =>
    `${p.container} can't download its image ${p.image ?? ""}.`,
  "sentence.diagnosis.imagePull.cause.unauthorized": (p: P) =>
    `${p.container} can't download its image: the registry refused the login.`,
  "sentence.diagnosis.imagePull.cause.rateLimited": (p: P) =>
    `${p.container} can't download its image: the registry limits how often it may.`,
  "sentence.diagnosis.imagePull.cause.unreachable": (p: P) =>
    `${p.container} can't download its image: the registry can't be reached.`,
  "sentence.diagnosis.imagePull.cause.invalidName": (p: P) =>
    `${p.container}'s image name ${p.image ?? ""} is wrong.`,
  "sentence.diagnosis.imagePull.cause.unknown": (p: P) =>
    `${p.container} can't download its image ${p.image ?? ""}.`,
  "sentence.diagnosis.imagePull.fix.notFound": () => "Check the image's name and version tag.",
  "sentence.diagnosis.imagePull.fix.notFoundOrPrivate": () =>
    "Check the name and version tag; if the image is private, give the project a login for its registry.",
  "sentence.diagnosis.imagePull.fix.unauthorized": () =>
    "Give the project a login for the registry (an imagePullSecret), or fix the one it has.",
  "sentence.diagnosis.imagePull.fix.rateLimited": () => "Sign in to the registry, or use a mirror.",
  "sentence.diagnosis.imagePull.fix.unreachable": () => "Check the machine's network and DNS.",
  "sentence.diagnosis.imagePull.fix.invalidName": () => "Fix the image name.",
  "sentence.diagnosis.imagePull.fix.unknown": () =>
    "The replica's events have the registry's answer.",
  "sentence.diagnosis.probeFailure.cause.liveness": (p: P) =>
    `${p.container}'s restart check failed ${p.count} times${p.output ? `: ${p.output}` : "."}`,
  "sentence.diagnosis.probeFailure.cause.readiness": (p: P) =>
    `${p.container}'s traffic check failed ${p.count} times${p.output ? `: ${p.output}` : "."}`,
  "sentence.diagnosis.probeFailure.cause.startup": (p: P) =>
    `${p.container} didn't finish starting in time.`,
  "sentence.diagnosis.probeFailure.fix.liveness": () =>
    "Check the restart check's path, port and timing in Health checks.",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "Check the traffic check's path, port and timing in Health checks.",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "If it just starts slowly, allow more time in Health checks; if it should be up by now, its logs say why.",
  "sentence.gap.resourceLimits.none": (p: P) =>
    `${p.container} sets no CPU or memory requests or limits.`,
  "sentence.gap.resourceLimits.partial": (p: P) =>
    `${p.container} doesn't set ${(p.missing ?? "").split(",").join(", ")}.`,
  "sentence.gap.healthChecks.none": (p: P) =>
    `${p.container} has no traffic check; traffic can reach it before it's ready.`,
  "sentence.gap.healthChecks.livenessOnly": (p: P) =>
    `${p.container} has a restart check but no traffic check.`,
  "sentence.gap.ingressToHttpRoute.retired": (p: P) =>
    `Ingress ${p.ingress} uses ${p.class}, whose controller is retired.`,
  "sentence.gap.ingressToHttpRoute.available": (p: P) =>
    `Ingress ${p.ingress} can move to an HTTPRoute; Gateway API is available.`,
  "sentence.gap.retiredIngressController": (p: P) =>
    `IngressClass ${p.class} is served by ${p.controller}, which is retired.`,
  "sentence.gap.blockedConnection": (p: P) =>
    p.at === "egress"
      ? `${p.from} calls ${p.to} (its variables say so), but its NetworkPolicy blocks the way out.`
      : p.at === "both"
        ? `${p.from} calls ${p.to} (its variables say so), but NetworkPolicy blocks both ends.`
        : `${p.from} calls ${p.to} (its variables say so), but ${p.to}'s NetworkPolicy doesn't let it in.`,
  "sentence.gap.noNetworkPolicy": (p: P) => {
    const blocked = cuts(
      p,
      {
        otherNamespaces: "traffic from other namespaces",
        outside: "traffic out of the cluster",
        published: "NodePort and LoadBalancer traffic",
        connections: "a connection between its services",
      },
      (items) =>
        items.length > 1 ? `${items.slice(0, -1).join(", ")} and ${items.at(-1)}` : items.join(""),
    )
    const services = (p.services ?? "").split(",")
    return [
      `No NetworkPolicy protects ${services.join(", ")}: any pod in the cluster can reach ${services.length > 1 ? "them" : "it"}.`,
      p.enforcement === "unknown" ? "Policies only work if the network plugin enforces them." : "",
      blocked ? `Isolating also blocks ${blocked}.` : "",
    ]
      .filter(Boolean)
      .join(" ")
  },
  "sentence.gap.volumeWithoutBackup": (p: P) => `Volume ${p.claim} has no backup schedule.`,
  "sentence.gap.databaseWithoutBackup": (p: P) => `Database ${p.database} has no backup schedule.`,
}

export type Messages = typeof en
