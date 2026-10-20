// English: the source of every message key. The other locales are typed as `Messages`, so a key
// missing there (or added there only) is a TypeScript error.
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

const objectEn = (p: P) => (p.object === "Secret" ? `secret ${p.name}` : `config ${p.name}`)

/** Who a volume's mount failure stops: the container, else the replica. */
const mountedEn = (p: P) => p.container ?? `Replica ${p.pod}`

/** Seconds in words, whole minutes as minutes. */
const secondsEn = (s: number) =>
  s >= 60 && s % 60 === 0
    ? `${s / 60} minute${s === 60 ? "" : "s"}`
    : `${s} second${s === 1 ? "" : "s"}`

/** A wait in its largest unit, rounded up: 90 s is 2 minutes. */
const waitEn = (s: number) =>
  s < 60
    ? plural(s, "second", "seconds")
    : s < 3600
      ? plural(Math.ceil(s / 60), "minute", "minutes")
      : plural(Math.ceil(s / 3600), "hour", "hours")

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
  "column.allServices": "All services",
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
  "cluster.attTight": (node: string, what: string, percent: string) =>
    `${node}: ${what} reserved at ${percent}`,
  "cluster.attNode": (node: string, state: string) => `${node}: ${state}`,
  "cluster.attComponent": (name: string, version: string, tested: string) =>
    `${name} ${version} is older than the tested ${tested}`,
  "cluster.noProjects": "No projects yet",
  "cluster.noProjectsBody":
    "A project is a Kubernetes namespace shown in Zeabur. Adopt one on the Projects page.",
  "cluster.openProjects": "Open projects",
  "cluster.nodes": "Nodes",
  "cluster.cpu": "CPU",
  "cluster.memory": "Memory",
  "cluster.cores": (n: string) => `${n} cores`,
  "cluster.requestedOf": (requested: string, allocatable: string) =>
    `${requested} of ${allocatable} reserved`,
  "cluster.allocatable": (allocatable: string) => `${allocatable} allocatable`,
  "cluster.used": (used: string) => `${used} used`,
  "cluster.limits": (limits: string) => `${limits} maximum`,
  "cluster.podsOf": (placed: string, room: string) => `${placed} of ${room} pods`,
  "cluster.pods": "Pods",
  "cluster.notReady": "Not ready",
  "cluster.cordoned": "Cordoned",
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
  "projects.noneBody":
    "A project is a Kubernetes namespace shown in Zeabur. Adopt one from the namespaces below.",
  "projects.showSystem": "Show system namespaces",
  "projects.forbiddenTitle": "You cannot list projects",
  "projects.view": "View",
  "projects.viewGrid": "Cards",
  "projects.viewList": "List",

  // Project
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
  "overview.updated": "Updated",
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

  // Architecture
  "lane.domains": "Domains",
  "lane.web": "Web",
  "lane.internal": "Private services",
  "lane.workers": "Workers & jobs",
  "lane.data": "Data",
  "edge.declared": "Declared",
  "edge.inferred": "Inferred",
  "edge.route": (ingress: string) => `Ingress ${ingress}`,
  "edge.shared": (kind: string, name: string) => `${kind} ${name}`,
  "edge.env": (variable: string) => variable,
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

  // Service page
  "service.tabs": "Service sections",
  "service.more": "More actions",
  "service.delete": "Delete service",
  "service.notFound": "This service does not exist.",
  "service.created": (date: string) => `Created ${date}`,
  "service.openTerminal": "Open terminal",
  "service.viewYaml": "View YAML",
  "tab.logs": "Logs",
  "tab.terminal": "Terminal",
  "tab.storage": "Storage",
  "tab.events": "Events",
  "tab.metrics": "Usage",
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
  "logs.unavailable.removed": "The cluster has removed this run's container and its log.",
  "logs.unavailableFix.no-previous-run": "Read the current run's log instead.",
  "logs.unavailableFix.not-started":
    "Wait for the container to start. The replica's events say what it is waiting for.",
  "logs.unavailableFix.removed":
    "Wait a moment and refresh. To keep logs past restarts, send them to a log store.",
  "logs.currentRun": "Show current run",
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
    "This container has no shell (distroless images include none). Open an ephemeral debug container instead:",
  "terminal.failed": "The container runtime ended the session.",
  "terminal.expired": "The session expired. Reconnect to continue.",
  "terminal.adminEnded": "Admin rights ended. Reconnect to continue with your own rights.",
  "terminal.idle": "Closed after 15 minutes without input or output.",
  "terminal.lost": "Connection lost.",
  "terminal.rbac":
    "Opening a terminal needs permission to run commands in this project's containers. Send this to an admin. It grants only that permission, and only in this project:",
  "terminal.roleLabel": "Role and RoleBinding",
  "terminal.debugLabel": "kubectl debug",

  // Variables
  "vars.title": "Environment variables",
  "vars.about": "Settings the app reads when it starts, such as a database address or an API key.",
  "vars.emptyTitle": "No environment variables yet",
  "vars.emptyBody": "Add one to change a setting without building a new image.",
  "vars.direct": "Set directly",
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
  "net.shortForm": "Short form within this project",
  "net.forwardsTo": (port: string) => `Forwarded to the container's port ${port}`,
  "net.exposeHint": "To add one, apply a Service or run this command.",
  "net.noneTitle": (name: string) => `${name} can't receive connections`,
  "net.noneBody":
    "It has no private address and no public domain. A worker that only makes outgoing connections needs neither, and can still reach other services and the internet.",
  "net.scheduledTitle": (name: string) => `${name} doesn't receive traffic`,
  "net.scheduledBody":
    "A job runs and exits, so nothing connects to it. It can still connect to other services and the internet.",
  "reach.about": "Which connections to and from this service the cluster allows.",
  "reach.fromDomains": "Public domains",
  "net.anyHost": "Any host",
  "net.defaultBackend": "Default backend",

  // Storage
  "storage.requested": "requested",
  "storage.title": "Volumes",
  "storage.about":
    "Files written to a volume stay when a replica restarts or moves to another node. Files anywhere else in the container are lost on restart.",
  "storage.emptyTitle": (name: string) => `${name} has no volume`,
  "storage.emptyBody": "Add one if the app must keep files, such as uploads or a database's data.",
  "storage.emptyJob": "To add one to a Job, edit its YAML.",
  "storage.bound": "Ready",
  "storage.pending": "Waiting for storage",
  "storage.lost": "Storage lost",
  "storage.mountPath": "Mount path",
  "storage.accessMode": "Access mode",
  "storage.class": "Storage class",
  "storage.defaultClass": "Default",
  "storage.otherMounts": "Other mounts",

  // Palette
  "palette.title": "Command palette",
  "palette.search": "Jump to a project, service, replica, or domain",
  "palette.recent": "Recent",
  "palette.actions": "Actions",
  "palette.openLogs": (name: string) => `Open logs of ${name}`,
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
  "action.saveDetails": "Save details",
  "title.adopt": (ns: string) => `Adopt ${ns}`,
  "title.unadopt": (ns: string) => `Stop showing ${ns} as a project`,
  "title.details": (ns: string) => `${ns} details`,
  "title.pauseAll": (ns: string) => `Pause every service in ${ns}`,
  "title.restartAll": (ns: string) => `Restart every service in ${ns}`,
  "confirm.adopt": "Adds the project label to the namespace. Nothing in it changes.",
  "confirm.unadopt": "The namespace stops showing as a project. Nothing in it is deleted.",
  "confirm.details": "Saves the name, description, and icon on the namespace.",
  "access.title": (ns: string) => `You can't change ${ns}`,
  "access.you": "<your user name>",
  "access.ask": (ns: string) =>
    `Ask a cluster administrator for permission to patch the namespace ${ns}.`,
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
    "Scales every running service to zero replicas. Services another tool manages are left out.",
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
  "objects.updated": "updated",
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
    `Delete ${keys} from ${kind} ${name}. The old value can't be restored here.`,
  "confirm.dataBoth": (set: string, removed: string, kind: string, name: string) =>
    `Set ${set} and delete ${removed} in ${kind} ${name}. A deleted value can't be restored here.`,
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
  "confirm.notRolledBack": "A rollback leaves ConfigMap and Secret contents as they are.",
  "confirm.noShrink": "A volume can't shrink after it grows.",
  "confirm.addVolume": (size: string, service: string) => `${service} gets a new ${size} volume.`,
  "confirm.moveMount": (service: string, from: string, to: string) =>
    `Move ${service}'s mount from ${from} to ${to}.`,
  "confirm.readOnly": (path: string, service: string) => `Make ${path} read-only in ${service}.`,
  "confirm.writable": (path: string, service: string) => `Make ${path} writable in ${service}.`,
  "confirm.mount": (service: string) => `Change a mount of ${service}.`,
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
  // Toasts
  "toast.region": "Notifications",
  "toast.applied": "Applied",
  "toast.view": "View",
  "toast.undo": "Undo",
  "toast.undone": "Undone",
  "toast.managed": (tool: string) => `Managed by ${tool}`,
  "toast.managedHelm": "Change it in the chart's values and upgrade the release.",
  "toast.managedGit": (tool: string) => `Change it in the Git repository ${tool} syncs from.`,
  "title.env": (service: string) => `Variables for ${service}`,
  "title.data": (kind: string, name: string) => `${kind} ${name}`,
  "title.restart": (service: string) => `Restart ${service}`,
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
  "title.compute": (service: string) => `Compute for ${service}`,
  "title.probe": (service: string) => `Health checks for ${service}`,
  "title.expand": (claim: string, size: string) => `Grow ${claim} to ${size}`,
  "title.mount": (path: string) => `Change mount ${path}`,
  "title.addVolume": (path: string) => `Add a volume at ${path}`,
  "releases.none": "No earlier releases yet.",
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
  "form.memoryRequest": "Reserved memory",
  "form.cpuLimit": "Maximum CPU",
  "form.memoryLimit": "Maximum memory",
  "form.computeHint":
    "The node sets the reserved amount aside for each replica. When a replica uses more memory than its maximum, its container restarts; CPU above the maximum is slowed down.",
  "form.largestNode": (cores: string, memory: string) => `Largest node: ${cores} cores, ${memory}`,
  "form.badQuantity": "Use a quantity such as 250m, 1 or 512Mi.",
  "form.badReplicas": "Enter a whole number from 0 to 1000.",
  "form.badNumber": "Enter a whole number above 0.",
  "form.badPath": "Enter an absolute path, such as /data.",
  "form.badName": "Use lowercase letters, digits, and -.",
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
  "form.size": "Size",
  "form.sizeHint": "A volume can grow but never shrink.",
  "form.expand": "Grow",
  "form.readOnly": "Read-only",
  "form.addVolume": "Add a volume",
  "form.volumeName": "Name",
  "form.statefulVolumes": "A StatefulSet's volumes are set when it is created.",
  "form.advanced": "Advanced",
  "form.claimName": "Claim name",
  "form.claimNameHint": "Creates a new claim; the cluster refuses a name already in use.",
  "form.badClaimName": "Use lowercase letters, digits, -, and ., at most 253 characters.",
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
  "data.restartAfter": "Restart readers after applying",
  "data.readers": "Read by",
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
  "gap.title": "Missing setup",
  "gap.warning": "Warning",
  "gap.advice": "Advice",
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
  "reach.allowFrom": (name: string) => `Allow from ${name}`,
  "reach.planIsolate": (name: string) => `Isolate ${name}`,
  "reach.planAllowIngress": (name: string) => `Let public domains reach ${name}`,
  "reach.planAllowFrom": (from: string, to: string) => `Let ${from} reach ${to}`,
  "reach.policies": "Rules",
  "reach.noPolicies": "This project has no connection rules, so every connection is allowed.",
  "reach.noneSelect": "No rule applies to this service, so every connection is allowed.",
  "reach.in": "Incoming",
  "reach.out": "Outgoing",
  "reach.otherNamespaces": "Other namespaces",
  "reach.dns": "Name lookups",
  "reach.outside": "Outside the cluster",
  "reach.cni": "Whether these rules are enforced depends on the cluster's network plugin (CNI).",
  "reach.allowed": "Allowed",
  "reach.limited": "Partly",
  "reach.blocked": "Blocked",
  "reach.edgeBlocked": "Blocked by a connection rule",
  "reach.edgeLimited": "Only partly allowed by connection rules",
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

  // Settings: the metrics store
  "settings.metricsStore": "Usage history",
  "settings.store": "History source",
  "settings.storeHint":
    "The Service that holds usage history, as namespace/name:port. Any store with the Prometheus HTTP API works. Zeabur reads it through the Kubernetes API with each person's own rights.",
  "settings.storeDefault": "Default",
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
  // Sentences the server names by key (diagnoses, gaps, problems); `writeSentence` finds them
  // under `sentence.<key>`.
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
          `The cluster checks every ${secondsEn(t.period)}; after ${t.threshold} failures in a row (about ${secondsEn(t.total)}) the container gets no traffic from its private addresses until it passes again.`,
        )
      : "While the traffic check fails, the container gets no traffic from its private addresses."
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
    `Nothing in the cluster can reach ${p.service} yet: it has no private address (Service).`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p: P) =>
    `${p.endpoint} sends to port ${p.targetPort}, which ${p.service}'s containers don't declare${p.ports ? ` (they declare ${p.ports})` : ""}.`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p: P) =>
    `${p.endpoint ?? p.address} has nowhere to send traffic: no replica of ${p.service} is ready.`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p: P) =>
    `Requests to the domain of ${p.route} don't reach ${p.service}: its entry point reports ${p.condition} ${p.reason}.`,
  "sentence.diagnosis.unreachable.why.noService": (p: P) =>
    p.from
      ? `${p.from} sends traffic to ${p.service} by name, but only a private address (Service) with that name can receive it.`
      : `A route sends traffic to ${p.endpoint ?? p.service} by name, but only a private address (Service) with that name can receive it.`,
  "sentence.diagnosis.unreachable.why.targetPort": () =>
    "A private address forwards to the port it names; a named port must be one the containers declare.",
  "sentence.diagnosis.unreachable.why.noReadyEndpoints": () =>
    "A private address sends traffic only to replicas that pass their traffic check.",
  "sentence.diagnosis.unreachable.why.routeNotAccepted": () =>
    "A domain's entry point must accept the route and find the address it names before requests go through.",
  "sentence.diagnosis.unreachable.fix.noService": () =>
    "Add a private address (Service) so other services or a domain can send it traffic.",
  "sentence.diagnosis.unreachable.fix.targetPort": () =>
    "Point the private address at a port the containers declare.",
  "sentence.diagnosis.unreachable.fix.noReadyEndpoints": () =>
    "Find out why no replica is ready: its other problems, its traffic check, its logs.",
  "sentence.diagnosis.unreachable.fix.routeNotAccepted": () =>
    "Check the route's entry point and the address it sends to; the route's status says which.",
  "sentence.diagnosis.unexplained.cause": (p: P) =>
    p.reason ? `${p.service} is failing (${p.reason}).` : `${p.service} is failing.`,
  "sentence.diagnosis.unexplained.why": (p: P) =>
    p.reason
      ? `Zeabur has no explanation for the state the cluster reports, ${p.reason}.`
      : "Zeabur has no explanation for the state the cluster reports.",
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
    "Check the image's entrypoint, command, and args.",
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
    "Check the restart check's path, port, and timing in Health checks.",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "Check the traffic check's path, port, and timing in Health checks.",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "If it only starts slowly, allow more time in Health checks; if it should be up by now, its logs say why.",
  "sentence.gap.resourceLimits.none": (p: P) =>
    `${p.container} sets no CPU or memory requests or limits.`,
  "sentence.gap.resourceLimits.partial": (p: P) =>
    `${p.container} doesn't set ${(p.missing ?? "").split(",").join(", ")}.`,
  "sentence.gap.healthChecks.none": (p: P) =>
    `${p.container} has no traffic check; traffic can reach it before it's ready.`,
  "sentence.gap.healthChecks.livenessOnly": (p: P) =>
    `${p.container} has a restart check but no traffic check.`,
  "sentence.gap.ingressToHttpRoute.retired": (p: P) =>
    `Ingress ${p.ingress} uses ${p.class}, whose controller is no longer maintained.`,
  "sentence.gap.ingressToHttpRoute.available": (p: P) =>
    `Gateway API is available, so Ingress ${p.ingress} can move to an HTTPRoute.`,
  "sentence.gap.retiredIngressController": (p: P) =>
    `IngressClass ${p.class} is served by ${p.controller}, which is no longer maintained.`,
  "sentence.gap.danglingRoute.noService": (p: P) => {
    const services = (p.services ?? "").split(",")
    const many = services.length > 1
    return `${p.kind} ${p.route} sends traffic to ${many ? "Services" : "Service"} ${services.join(", ")}, which ${many ? "don't" : "doesn't"} exist in this project.`
  },
  "sentence.gap.danglingRoute.noGateway": (p: P) => {
    const gateways = (p.gateways ?? "").split(",")
    const many = gateways.length > 1
    return `HTTPRoute ${p.route} attaches to ${many ? "Gateways" : "Gateway"} ${gateways.join(", ")}, which ${many ? "don't" : "doesn't"} exist.`
  },
  "sentence.gap.danglingRoute.noParent": (p: P) =>
    `HTTPRoute ${p.route} names no Gateway, so it receives no traffic.`,
  "sentence.gap.danglingRoute.notAccepted": (p: P) =>
    `No Gateway has accepted HTTPRoute ${p.route}${p.reason ? ` (${p.reason})` : ""}, so it receives no traffic.`,
  "sentence.gap.blockedConnection": (p: P) =>
    p.at === "egress"
      ? `${p.from}'s variables point to ${p.to}, but ${p.from}'s own NetworkPolicy blocks the outgoing connection.`
      : p.at === "both"
        ? `${p.from}'s variables point to ${p.to}, but NetworkPolicy blocks the connection at both ends.`
        : `${p.from}'s variables point to ${p.to}, but ${p.to}'s NetworkPolicy doesn't let ${p.from} in.`,
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
  // Problems the page makes itself
  "sentence.page.noAnswer": () => "Zeabur didn't respond.",
  "sentence.page.status": (p: P) => `The request failed with HTTP ${p.status}.`,
  "sentence.page.reload": () => "Check that Zeabur is still running, then reload the page.",
  "sentence.page.followFailed": () => "Unable to follow the log.",
  "sentence.page.followOff": () => "Turn off Follow; the snapshot shows the cluster's answer.",
  "status.readyUnchecked": "Running (no traffic check)",
  "origin.code": "In the app",
  "origin.configuration": "In its settings",
  "origin.cluster": "In the cluster",
  "origin.upstream": "Outside the cluster",
  "origin.unknown": "Cause unknown",
  "origin.code.hint": "The app's own code or command stops it; its logs say why.",
  "origin.configuration.hint":
    "A setting of this service causes it: a variable, a limit, a check or an address.",
  "origin.cluster.hint": "The cluster under the service causes it: its nodes, quotas or room.",
  "origin.upstream.hint": "Something outside the cluster causes it, such as an image registry.",
  "origin.unknown.hint":
    "Zeabur can't tell yet what causes it; the events and logs have the details.",
  "fixer.you": "You can fix this",
  "fixer.clusterAdmin": "Needs a cluster admin",
  "fixer.unknown": "Fixer unknown",
  "diagnosis.copy": "Copy diagnosis",
  "diagnosis.lastSeen": "Last seen",
  "trace.title": "Path of a request",
  "status.uncheckedWhy": "Its containers started, but nothing checks that it answers requests.",
  "status.addTrafficCheck": "Add a traffic check",
  "status.reach": "How it's reached",
  "status.reachNone":
    "Nothing reaches this service: it has no public domain and no private address.",
  "status.private": "Private",
  "rollout.title": "Rollout",
  "rollout.ready": (ready: number, desired: number) => `${ready} of ${desired} new replicas ready`,
  "rollout.older": (n: number) =>
    plural(n, "old replica still running", "old replicas still running"),
  "changes.title": "What changed",
  "changes.description":
    "Each release of the service and the warnings around it. Compare a release with the current one, or roll back to it.",
  "changes.none": "No changes yet",
  "changes.release": (revision: number) => `Release #${revision}`,
  "configure.sections": "Settings of this service",
  "configure.variables": "Variables",
  "configure.networking": "Domains and addresses",
  "configure.storage": "Storage",
  "configure.compute": "Compute",
  "configure.health-checks": "Health checks",
  "form.healthHint":
    "Checks the cluster runs on each replica: whether it takes traffic, whether to restart it, and when it has started.",
  "draft.add": "Add to changes",
  "draft.pendingHere": "has changes waiting for review",
  "draft.count": (n: number) => plural(n, "change pending", "changes pending"),
  "draft.review": "Review",
  "draft.discard": "Discard",
  "draft.title": (name: string) => `Change ${name}`,
  "section.status": "Status",
  "section.changes": "Changes",
  "section.configure": "Configure",
  "section.inspect": "Inspect",
  "inspect.views": "What to inspect",
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
  "freshness.lastRead": "Last read",
  "home.state.fine": (services: number, projects: number) =>
    `All ${services} services in ${plural(projects, "project", "projects")} are running.`,
  "home.state.problems": (n: number) =>
    n === 1 ? "1 problem needs attention." : `${n} problems need attention.`,
  "home.state.rolling": (n: number) =>
    n === 1 ? "1 service is rolling out." : `${n} services are rolling out.`,
  "home.affects": "Affects",
  "home.inProgress": "In progress",
  "home.problems": "What needs you",
  "shell.context": (cluster: string, user: string) => `Cluster ${cluster}, signed in as ${user}`,
  "home.clusterNotices": (n: number) =>
    n === 1
      ? "1 cluster notice, nothing down because of it:"
      : `${n} cluster notices, nothing down because of them:`,
  "origin.component": "In a cluster component",
  "origin.component.hint":
    "A component the cluster runs for every project causes it, such as the ingress controller or cluster DNS.",
  "origin.zeabur": "In Zeabur",
  "origin.zeabur.hint": "Zeabur itself causes it; report it with the copied diagnosis.",
  "fixer.componentOwner": "Needs the component's owner",
  "incident.title": "In the cluster",
  "incident.hint": "One cause takes down several services; fixing it fixes them all.",
  "incident.domains": (n: number) => plural(n, "domain", "domains"),
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
  "trace.someUnknown": "Some hops could not be checked; hover one to see why.",
  "rollout.rollBack": (name: string) => `Roll back ${name}`,
  "rollout.rollBackButton": "Roll back",
  "sentence.incident.componentDown.cause.ingressController": (p: P) =>
    `The ingress controller ${p.workload} is down: none of its replicas is ready.`,
  "sentence.incident.componentDown.cause.certManager": (p: P) =>
    `cert-manager is down: no replica of ${p.workload} is ready.`,
  "sentence.incident.componentDown.cause.clusterDns": (p: P) =>
    `Cluster DNS is down: no replica of ${p.workload} is ready.`,
  "sentence.incident.componentDown.cause.metricsServer": (p: P) =>
    `metrics-server is down: no replica of ${p.workload} is ready.`,
  "sentence.incident.componentDown.why.ingressController": (p: P) =>
    `The ingress controller brings requests for public domains to the services they route to; while it is down, ${plural(Number(p.domains), "domain", "domains")} and ${plural(Number(p.services), "service", "services")} get no requests from outside the cluster.`,
  "sentence.incident.componentDown.why.certManager": (p: P) =>
    `cert-manager issues and renews the certificates domains ask it for; while it is down, the certificates of ${plural(Number(p.domains), "domain", "domains")} are neither issued nor renewed, and browsers refuse a certificate once it expires.`,
  "sentence.incident.componentDown.why.clusterDns": (p: P) =>
    `Services find each other by name through cluster DNS; while it is down, ${plural(Number(p.services), "service that calls", "services that call")} another by name can't reach it.`,
  "sentence.incident.componentDown.why.metricsServer": () =>
    "metrics-server measures what each replica uses; while it is down, usage figures are missing and autoscaling stops.",
  "sentence.incident.componentDown.fix.ingressController": (p: P) =>
    `Check why ${p.workload}${p.namespace ? ` in ${p.namespace}` : ""} has no ready replica; its events and logs say why.`,
  "sentence.incident.componentDown.fix.certManager": (p: P) =>
    `Check why ${p.workload}${p.namespace ? ` in ${p.namespace}` : ""} has no ready replica; its events and logs say why.`,
  "sentence.incident.componentDown.fix.clusterDns": (p: P) =>
    `Check why ${p.workload}${p.namespace ? ` in ${p.namespace}` : ""} has no ready replica; its events and logs say why.`,
  "sentence.incident.componentDown.fix.metricsServer": (p: P) =>
    `Check why ${p.workload}${p.namespace ? ` in ${p.namespace}` : ""} has no ready replica; its events and logs say why.`,
  "sentence.incident.nodeNotReady.cause": (p: P) =>
    `${p.status === "Unknown" ? `Node ${p.node} stopped reporting to the cluster` : `Node ${p.node} is not ready`}${p.reason ? ` (${p.reason})` : ""}.`,
  "sentence.incident.nodeNotReady.why": (p: P) =>
    `A node that isn't ready starts nothing new, and after a grace period, 5 minutes by default, the cluster removes its pods so they can start elsewhere. ${plural(Number(p.pods), "pod runs", "pods run")} on it; ${plural(Number(p.services), "service has", "services have")} replicas there.`,
  "sentence.incident.nodeNotReady.fix": (p: P) =>
    `Check the machine behind ${p.node}: whether it is on, its network, and its node agent (kubelet). If it won't come back, drain it and replace it.`,
  "sentence.trace.dns.ok": (p: P) => `${p.host} resolves.`,
  "sentence.trace.dns.notFound": (p: P) =>
    `${p.host} doesn't resolve from where Zeabur runs; DNS elsewhere may answer differently.`,
  "sentence.trace.dns.unknown": (p: P) =>
    `${p.host} wasn't looked up, or the lookup didn't answer in time.`,
  "sentence.trace.address.noStatus": (p: P) =>
    `${p.route} has no address in its status yet, so the domain's address can't be compared with it.`,
  "sentence.trace.address.unresolved": (p: P) =>
    `The domain didn't resolve, so its address can't be compared with ${p.route}'s.`,
  "sentence.trace.address.ok": (p: P) =>
    `The domain points to the address ${p.route} is served on.`,
  "sentence.trace.address.elsewhere": (p: P) =>
    `The domain points to another address than ${p.route}'s; a CDN or proxy in front of the cluster answers with its own.`,
  "sentence.trace.controller.classesUnreadable": (p: P) =>
    `Zeabur can't read the cluster's IngressClasses, so it can't tell which controller serves ${p.route}.`,
  "sentence.trace.controller.classMissing": (p: P) =>
    `${p.route} asks for IngressClass ${p.class}, which the cluster doesn't have.`,
  "sentence.trace.controller.unreadable": (p: P) =>
    `Zeabur can't read the cluster's components, so it can't tell whether the ingress controller of ${p.route} runs.`,
  "sentence.trace.controller.notRecognised": (p: P) =>
    `Zeabur doesn't recognize the ingress controller${p.controller ? ` ${p.controller}` : ""} that serves ${p.route}, so it can't tell whether it runs.`,
  "sentence.trace.controller.ok": (p: P) =>
    `The ingress controller ${p.workload} has a ready replica.`,
  "sentence.trace.controller.down": (p: P) =>
    `The ingress controller ${p.workload} has no ready replica, so nothing serves ${p.route}.`,
  "sentence.trace.certificate.defaultCertificate": (p: P) =>
    `${p.route} lists ${p.host} for HTTPS without a secret, so the controller serves its default certificate.`,
  "sentence.trace.certificate.ok": (p: P) =>
    `The certificate for ${p.host} is issued${p.until ? `, valid until ${p.until}` : ""}.`,
  "sentence.trace.certificate.notReady": (p: P) =>
    `Certificate ${p.certificate} for ${p.host} isn't issued${p.reason ? ` (${p.reason})` : ""}.`,
  "sentence.trace.certificate.pending": (p: P) =>
    `Certificate ${p.certificate} for ${p.host} has no result yet.`,
  "sentence.trace.certificate.unreadable": (p: P) =>
    `Zeabur can't read the project's secrets, so it can't check the certificate for ${p.host} in secret ${p.secret}.`,
  "sentence.trace.certificate.notChecked": (p: P) =>
    `Secret ${p.secret} holds the certificate for ${p.host}; Zeabur doesn't check it further.`,
  "sentence.trace.certificate.secretMissing": (p: P) =>
    `${p.route} reads the certificate for ${p.host} from secret ${p.secret}, which doesn't exist; the controller serves its default certificate, which browsers refuse.`,
  "sentence.trace.route.noStatus": (p: P) =>
    `Nothing has written ${p.route}'s status yet, so whether it is taken up is unknown.`,
  "sentence.trace.route.ok": (p: P) =>
    `${p.route} is taken up: its controller wrote an address into its status.`,
  "sentence.trace.route.accepted": (p: P) => `A Gateway accepted ${p.route}.`,
  "sentence.trace.route.notAccepted": (p: P) =>
    `${p.route}'s entry point reports ${p.condition} ${p.reason}; requests don't go through.`,
  "sentence.trace.service.unreadable": (p: P) =>
    `Zeabur can't read the project's private addresses (Services), so it can't check ${p.endpoint}.`,
  "sentence.trace.service.missing": (p: P) =>
    `No private address (Service) is named ${p.endpoint}.`,
  "sentence.trace.service.portMissing": (p: P) =>
    `Private address ${p.endpoint} has no port ${p.port}.`,
  "sentence.trace.service.selectsOther": (p: P) =>
    `Private address ${p.endpoint} sends to other replicas, not ${p.service}'s.`,
  "sentence.trace.service.ok": (p: P) => `Private address ${p.endpoint} sends to ${p.service}.`,
  "sentence.trace.targetPort.unknown": (p: P) =>
    `Zeabur couldn't read the containers, so the port ${p.endpoint} sends to isn't checked.`,
  "sentence.trace.targetPort.notReached": (p: P) =>
    `Not checked: traffic stops before it reaches a port of ${p.endpoint}.`,
  "sentence.trace.targetPort.mismatch": (p: P) =>
    `${p.endpoint} sends to port ${p.targetPort}, which the containers don't declare${p.ports ? ` (they declare ${p.ports})` : ""}.`,
  "sentence.trace.targetPort.undeclared": (p: P) =>
    `The containers declare no ports, so whether they listen where ${p.endpoint} sends is unknown.`,
  "sentence.trace.targetPort.ok": (p: P) => `${p.endpoint} sends to a port the containers declare.`,
  "sentence.trace.ready.scaledToZero": (p: P) => `${p.service} is scaled to 0 replicas.`,
  "sentence.trace.ready.ok": (p: P) => `${p.service} has ${p.ready}/${p.pods} replicas ready.`,
  "sentence.trace.ready.starting": (p: P) => `${p.service} is starting; no replica is ready yet.`,
  "sentence.trace.ready.noneReady": (p: P) => `No replica of ${p.service} is ready.`,
  "sentence.diagnosis.unreachable.cause.dnsNotFound": (p: P) =>
    `Requests to ${p.host} don't reach ${p.service}: the domain doesn't resolve.`,
  "sentence.diagnosis.unreachable.why.dnsNotFound": () =>
    "A browser first looks a domain up in DNS. Zeabur looked it up from where it runs; DNS elsewhere may answer differently.",
  "sentence.diagnosis.unreachable.fix.dnsNotFound": (p: P) =>
    `Add a DNS record for ${p.host} that points to the cluster's ingress controller.`,
  "sentence.diagnosis.unreachable.cause.ingressClassMissing": (p: P) =>
    `Requests to the domain of ${p.route} don't reach ${p.service}: it asks for IngressClass ${p.class}, which the cluster doesn't have.`,
  "sentence.diagnosis.unreachable.why.ingressClassMissing": () =>
    "An ingress controller serves only the Ingresses of its own class; with no such class, none serves this one.",
  "sentence.diagnosis.unreachable.fix.ingressClassMissing": (p: P) =>
    `Set ${p.route}'s class to one the cluster has, or install the controller for ${p.class}.`,
  "sentence.diagnosis.unreachable.cause.controllerDown": (p: P) =>
    `Requests to the domain of ${p.route} don't reach ${p.service}: the ingress controller ${p.workload} has no ready replica.`,
  "sentence.diagnosis.unreachable.why.controllerDown": () =>
    "The ingress controller brings requests for a domain into the cluster; while none of its replicas is ready, nothing answers them.",
  "sentence.diagnosis.unreachable.fix.controllerDown": (p: P) =>
    `Check why ${p.workload}${p.namespace ? ` in ${p.namespace}` : ""} has no ready replica; its events and logs say why.`,
  "sentence.diagnosis.unreachable.cause.certificateMissing": (p: P) =>
    `HTTPS to ${p.host} fails: secret ${p.secret}, which should hold its certificate, doesn't exist.`,
  "sentence.diagnosis.unreachable.why.certificateMissing": (p: P) =>
    `Without the secret ${p.route} names, the controller serves its default certificate, which browsers refuse.`,
  "sentence.diagnosis.unreachable.fix.certificateMissing": (p: P) =>
    `Add secret ${p.secret} with the certificate for ${p.host}, or have cert-manager issue it.`,
  "sentence.diagnosis.unreachable.cause.certificateNotReady": (p: P) =>
    `HTTPS to ${p.host} fails: certificate ${p.certificate} isn't issued${p.reason ? ` (${p.reason})` : ""}.`,
  "sentence.diagnosis.unreachable.why.certificateNotReady": (p: P) =>
    `cert-manager writes the certificate into secret ${p.secret} once it is issued; until then the controller serves its default certificate, which browsers refuse.`,
  "sentence.diagnosis.unreachable.fix.certificateNotReady": (p: P) =>
    `Certificate ${p.certificate}'s status and events say why it isn't issued.`,
  "sentence.diagnosis.unreachable.cause.backendPortMissing": (p: P) =>
    `A route to ${p.service} names port ${p.port}, which its private address ${p.endpoint} doesn't have.`,
  "sentence.diagnosis.unreachable.why.backendPortMissing": () =>
    "A route sends to one port of a private address, by number or name; a port the address doesn't have receives nothing.",
  "sentence.diagnosis.unreachable.fix.backendPortMissing": (p: P) =>
    `Point the route at a port ${p.endpoint} has, or add port ${p.port} to ${p.endpoint}.`,
  "sentence.diagnosis.unreachable.cause.selectsOther": (p: P) =>
    `A route to ${p.service} sends to ${p.endpoint}, which sends to other replicas, not ${p.service}'s.`,
  "sentence.diagnosis.unreachable.why.selectsOther": () =>
    "A private address sends to the replicas whose labels match its selector.",
  "sentence.diagnosis.unreachable.fix.selectsOther": (p: P) =>
    `Make ${p.endpoint}'s selector match ${p.service}'s labels, or point the route at ${p.service}'s own private address.`,
  "sentence.diagnosis.unreachable.cause.scaledToZero": (p: P) =>
    `Nothing reaches ${p.service}: it is scaled to 0 replicas.`,
  "sentence.diagnosis.unreachable.why.scaledToZero": () =>
    "A private address sends only to running replicas, and none is set to run.",
  "sentence.diagnosis.unreachable.fix.scaledToZero": (p: P) =>
    `Scale ${p.service} to at least 1 replica.`,
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
  "sentence.rollout.stuck.deadlineExceeded": (p: P) =>
    `The rollout of ${p.name} is stuck: it made no progress before its deadline (progressDeadlineSeconds).`,
  "sentence.rollout.stuck.replicaFailure": (p: P) =>
    `The rollout of ${p.name} is stuck: the cluster can't create a new replica.`,
  "sentence.gap.ephemeralData.none": (p: P) =>
    `${p.container} keeps its ${p.software} data in ${p.path}, which is on no volume; it's lost whenever the replica is replaced.`,
  "sentence.gap.ephemeralData.ephemeral": (p: P) =>
    `${p.container} keeps its ${p.software} data in ${p.path}, on a volume that lasts only as long as the replica; it's lost whenever the replica is replaced.`,
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
  "preview.blocked": "Why it does not fit",
  "preview.blockedQuota": (name: string, resource: string, need: string, left: string) =>
    `Quota ${name} blocks it: ${resource} needs ${need}, ${left} is left.`,
  "preview.blockedNodes": (_name: string, resource: string, need: string, left: string) =>
    `No node has room: ${resource} needs ${need}, the roomiest node has ${left}.`,
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
  "env.pendingRestartDetail": (pod: string) =>
    `Changed after ${pod} started. Restart the service to use the new value.`,
  // ia:end
}

export type Messages = typeof en
