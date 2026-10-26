import { durationOf, items, joinWords, type P } from "@/i18n/sentence-parts"

const seconds = (n: number) => `${n} ${n === 1 ? "second" : "seconds"}`
const minutes = (n: number) => `${n} ${n === 1 ? "minute" : "minutes"}`
const words = (list: string[], and = false) => joinWords(list, ", ", and ? " and " : ", ")
const duration = (value: string | undefined) => durationOf(value, seconds, minutes)

export const serverEn = {
  "sentence.diagnosis.crashLoop.why.backOff": (p: P) =>
    `It has restarted ${p.restarts ?? ""} times; the cluster starts it again each time, waiting longer between tries, up to 5 minutes by default.`,
  "sentence.diagnosis.crashLoop.why.exitsZero": (p: P) =>
    `A service is meant to keep running, so the cluster starts it again whenever its program ends, even cleanly (${p.restarts ?? ""} times so far).`,
  "sentence.diagnosis.runFailed.why.exitCode": (p: P) =>
    `A run counts as failed when its program ends with an exit code other than 0; this one ended with ${p.exitCode ?? ""}.`,
  "sentence.diagnosis.runFailed.why.gaveUp": (p: P) =>
    `${!p.backoffLimit || p.backoffLimit === "none" ? `The task gave up after its retries; the last run ended with exit code ${p.exitCode ?? ""}.` : `${p.backoffLimit === "0" ? `The task doesn't retry a failed run; this one ended with exit code ${p.exitCode ?? ""}.` : `The task gave up: it retries a failed run ${p.backoffLimit ?? ""} times, and the last one ended with exit code ${p.exitCode ?? ""} too.`}`}`,
  "sentence.diagnosis.outOfMemory.why.limit": (p: P) =>
    `Each replica may use at most ${p.limit ?? ""} of memory; at that point the system stops it at once, and the program can't catch it.`,
  "sentence.diagnosis.outOfMemory.why.noLimit": () =>
    "With no memory maximum, it can use the machine's memory until the machine runs short; then the system stops what uses the most.",
  "sentence.diagnosis.imagePull.why": () =>
    "The cluster downloads the image before it starts the container, and keeps retrying, waiting longer each time, until the download works.",
  "sentence.diagnosis.probeFailure.why.liveness": (p: P) =>
    `${!p.period || p.period === "none" ? `When the restart check fails several times in a row, the cluster restarts the container.` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `By default, the cluster checks every ${duration(p.period)} and restarts the container after ${p.threshold ?? ""} failures in a row (about ${duration(p.total)}).` : `The cluster checks every ${duration(p.period)} and restarts the container after ${p.threshold ?? ""} failures in a row (about ${duration(p.total)}). Every ${duration(p.period)} is the default.`}` : `The cluster checks every ${duration(p.period)} and restarts the container after ${p.threshold ?? ""} failures in a row (about ${duration(p.total)}).${p.thresholdDefault === "true" ? ` ${p.threshold ?? ""} failures is the default.` : ``}`}`}`,
  "sentence.diagnosis.probeFailure.why.readiness": (p: P) =>
    `${!p.period || p.period === "none" ? `While the traffic check fails, the container gets no traffic from its private addresses.` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `By default, the cluster checks every ${duration(p.period)}; after ${p.threshold ?? ""} failures in a row (about ${duration(p.total)}) the container gets no traffic from its private addresses until it passes again.` : `The cluster checks every ${duration(p.period)}; after ${p.threshold ?? ""} failures in a row (about ${duration(p.total)}) the container gets no traffic from its private addresses until it passes again. Every ${duration(p.period)} is the default.`}` : `The cluster checks every ${duration(p.period)}; after ${p.threshold ?? ""} failures in a row (about ${duration(p.total)}) the container gets no traffic from its private addresses until it passes again.${p.thresholdDefault === "true" ? ` ${p.threshold ?? ""} failures is the default.` : ``}`}`}`,
  "sentence.diagnosis.probeFailure.why.startup": (p: P) =>
    `${!p.period || p.period === "none" ? `The other checks wait for the startup check; if it doesn't pass in time, the cluster restarts the container.` : `${p.periodDefault === "true" ? `${p.thresholdDefault === "true" ? `By default, the container has ${p.threshold ?? ""} × ${duration(p.period)} (${duration(p.total)}) to pass the startup check; then the cluster restarts it.` : `The container has ${p.threshold ?? ""} × ${duration(p.period)} (${duration(p.total)}) to pass the startup check; then the cluster restarts it. Every ${duration(p.period)} is the default.`}` : `The container has ${p.threshold ?? ""} × ${duration(p.period)} (${duration(p.total)}) to pass the startup check; then the cluster restarts it.${p.thresholdDefault === "true" ? ` ${p.threshold ?? ""} failures is the default.` : ``}`}`}`,
  "sentence.diagnosis.configMissing.cause.object": (p: P) =>
    `${p.container ?? ""} can't start: ${p.object === "Secret" ? `secret ${p.name ?? ""}` : `config file ${p.name ?? ""}`} doesn't exist.`,
  "sentence.diagnosis.configMissing.cause.key": (p: P) =>
    `${p.container ?? ""} can't start: ${p.object === "Secret" ? `secret ${p.name ?? ""}` : `config file ${p.name ?? ""}`} has no ${p.key ?? ""}.`,
  "sentence.diagnosis.configMissing.cause.unknown": (p: P) =>
    `${p.container ?? ""} can't start: a secret or config it reads is missing.`,
  "sentence.diagnosis.configMissing.cause.volume": (p: P) =>
    `${!p.container || p.container === "none" ? `Replica ${p.pod ?? ""}` : `${p.container ?? ""}`} can't start: volume ${p.volume ?? ""} comes from ${p.object === "Secret" ? `secret ${p.name ?? ""}` : `config file ${p.name ?? ""}`}, which doesn't exist.`,
  "sentence.diagnosis.configMissing.why": () =>
    "The cluster starts a container only once every secret and config it reads, and every key it names, exists.",
  "sentence.diagnosis.configMissing.why.volume": () =>
    "The cluster mounts every volume before it starts a replica's containers; a volume from a secret or config that doesn't exist can't be mounted, so they keep waiting.",
  "sentence.diagnosis.configMissing.fix.object": (p: P) =>
    `Add ${p.object === "Secret" ? `secret ${p.name ?? ""}` : `config file ${p.name ?? ""}`} to the project, or stop reading it.`,
  "sentence.diagnosis.configMissing.fix.key": (p: P) =>
    `Add ${p.key ?? ""} to ${p.object === "Secret" ? `secret ${p.name ?? ""}` : `config file ${p.name ?? ""}`}, or stop reading it.`,
  "sentence.diagnosis.configMissing.fix.unknown": () =>
    "The replica's events name what is missing; add it, or stop reading it.",
  "sentence.diagnosis.configMissing.fix.volume": (p: P) =>
    `Add ${p.object === "Secret" ? `secret ${p.name ?? ""}` : `config file ${p.name ?? ""}`} to the project, or remove volume ${p.volume ?? ""}.`,
  "sentence.diagnosis.unschedulable.cause.cpu": (p: P) =>
    `${p.service ?? ""} is waiting for room: no machine has enough CPU free.`,
  "sentence.diagnosis.unschedulable.cause.memory": (p: P) =>
    `${p.service ?? ""} is waiting for room: no machine has enough memory free.`,
  "sentence.diagnosis.unschedulable.cause.placement": (p: P) =>
    `${p.service ?? ""} is waiting for a machine it is allowed to run on.`,
  "sentence.diagnosis.unschedulable.cause.other": (p: P) =>
    `${p.service ?? ""} is waiting for a machine to run on.`,
  "sentence.diagnosis.unschedulable.why": (p: P) =>
    `Each replica goes on a machine with room for what it reserves; the cluster says: ${p.message ?? ""}`,
  "sentence.diagnosis.unschedulable.cause.claimMissing": (p: P) =>
    `${p.service ?? ""} can't start: the volume ${p.claim ?? ""} it mounts doesn't exist.`,
  "sentence.diagnosis.unschedulable.cause.claimUnbound": (p: P) =>
    `${p.service ?? ""} can't start: its volume isn't ready yet.`,
  "sentence.diagnosis.unschedulable.whyClaim": (p: P) =>
    `A replica starts only once its volumes are ready; the cluster says: ${p.message ?? ""}`,
  "sentence.diagnosis.unschedulable.fix.claimMissing": (p: P) =>
    `Create a volume named ${p.claim ?? ""}, or change the service to use one that exists.`,
  "sentence.diagnosis.unschedulable.fix.claimUnbound": () =>
    "The volume's events say why it has no storage yet; often the cluster has no default storage class.",
  "sentence.diagnosis.unschedulable.fix.cpu": () =>
    "Reserve less CPU, run fewer replicas, or add a machine.",
  "sentence.diagnosis.unschedulable.fix.memory": () =>
    "Reserve less memory, run fewer replicas, or add a machine.",
  "sentence.diagnosis.unschedulable.fix.placement": () =>
    "Check where it is allowed to run (node selector, affinity, tolerations).",
  "sentence.diagnosis.unschedulable.fix.other": () =>
    "The replica's events have the scheduler's full answer.",
  "sentence.diagnosis.quotaBlocked.cause": (p: P) =>
    `${p.service ?? ""} can't start more replicas: the project's limit ${p.quota ?? ""} is reached.`,
  "sentence.diagnosis.quotaBlocked.why": (p: P) =>
    `The project limit ${p.quota ?? ""} caps what all its services may reserve together; the cluster refuses new replicas over it.`,
  "sentence.diagnosis.quotaBlocked.fix": () =>
    "Run fewer replicas, reserve less per replica, or ask an admin to raise the limit.",
  "sentence.diagnosis.podSecurity.cause.runsAsRoot": (p: P) =>
    `${p.container ?? ""} can't start: its image runs as root, and the service's security is Strict.`,
  "sentence.diagnosis.podSecurity.cause.nonNumericUser": (p: P) =>
    `${p.container ?? ""} can't start: the service's security is Strict, and its image names its user by name, so the cluster can't tell it isn't root.`,
  "sentence.diagnosis.podSecurity.cause.refused": (p: P) =>
    `${p.service ?? ""} can't start replicas: the project's Pod Security level ${p.level ?? ""} refuses them.`,
  "sentence.diagnosis.podSecurity.why.runAsNonRoot": () =>
    "Strict security tells the cluster never to start a container as root. It checks the user the image runs as, and refuses root or a user it can't check.",
  "sentence.diagnosis.podSecurity.why.refused": (p: P) =>
    `The cluster refuses replicas that break the project's ${p.level ?? ""} level: ${p.breaks ?? ""}.`,
  "sentence.diagnosis.podSecurity.fix.compatible": () =>
    "Choose Compatible security in the service's settings. To keep Strict, use an image that runs without root, such as nginxinc/nginx-unprivileged instead of nginx, or put a numeric user such as USER 1000 in your Dockerfile.",
  "sentence.diagnosis.podSecurity.fix.numericUser": () =>
    "Give the image a numeric user (USER 101, not USER nginx), or choose Compatible security in the service's settings.",
  "sentence.diagnosis.podSecurity.fix.strict": () =>
    "The project allows only Strict services. Make the service Strict in its settings, or ask an admin to set the project's Pod Security level to baseline.",
  "sentence.diagnosis.podSecurity.fix.privileged": (p: P) =>
    `The project's level ${p.level ?? ""} doesn't allow what the service asks for; remove that setting, or ask an admin to allow it.`,
  "sentence.diagnosis.unreachable.cause.noService": (p: P) =>
    `Nothing in the cluster can reach ${p.service ?? ""} yet: it has no private address (Service).`,
  "sentence.diagnosis.unreachable.cause.targetPort": (p: P) =>
    `${p.endpoint ?? ""} sends to port ${p.targetPort ?? ""}, which ${p.service ?? ""}'s containers don't declare${!p.ports || p.ports === "none" ? `` : ` (they declare ${p.ports ?? ""})`}.`,
  "sentence.diagnosis.unreachable.cause.noReadyEndpoints": (p: P) =>
    `${!p.endpoint || p.endpoint === "none" ? `${p.address ?? ""}` : `${p.endpoint ?? ""}`} has nowhere to send traffic: no replica of ${p.service ?? ""} is ready.`,
  "sentence.diagnosis.unreachable.cause.routeNotAccepted": (p: P) =>
    `Requests to the domain of ${p.route ?? ""} don't reach ${p.service ?? ""}: its entry point reports ${p.condition ?? ""} ${p.reason ?? ""}.`,
  "sentence.diagnosis.unreachable.why.noService": (p: P) =>
    `${!p.from || p.from === "none" ? `A route sends traffic to ${!p.endpoint || p.endpoint === "none" ? `${p.service ?? ""}` : `${p.endpoint ?? ""}`} by name, but only a private address (Service) with that name can receive it.` : `${p.from ?? ""} sends traffic to ${p.service ?? ""} by name, but only a private address (Service) with that name can receive it.`}`,
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
  "sentence.diagnosis.unexplained.cause.waiting": (p: P) =>
    `${p.stuck === "true" ? `The rollout of ${p.service ?? ""} is stuck: the new replica ${p.pod ?? ""} is waiting (${p.reason ?? ""}).` : `${p.pod ?? ""} isn't running: ${p.container ?? ""} is waiting (${p.reason ?? ""}).`}`,
  "sentence.diagnosis.unexplained.cause.terminated": (p: P) =>
    `${p.stuck === "true" ? `The rollout of ${p.service ?? ""} is stuck: the new replica ${p.pod ?? ""} stopped (${p.reason ?? ""}).` : `${p.pod ?? ""} isn't running: ${p.container ?? ""} stopped (${p.reason ?? ""}).`}`,
  "sentence.diagnosis.unexplained.cause.pending": (p: P) =>
    `${p.stuck === "true" ? `The rollout of ${p.service ?? ""} is stuck: the new replica ${p.pod ?? ""} hasn't started (Pending).` : `${p.pod ?? ""} hasn't started (Pending).`}`,
  "sentence.diagnosis.unexplained.cause.notReady": (p: P) =>
    `${p.stuck === "true" ? `The rollout of ${p.service ?? ""} is stuck: the new replica ${p.pod ?? ""} ${!p.check || p.check === "none" ? `runs but isn't ready` : `runs but doesn't pass its traffic check (${p.check ?? ""})`}.` : `${p.pod ?? ""} ${!p.check || p.check === "none" ? `runs but isn't ready` : `runs but doesn't pass its traffic check (${p.check ?? ""})`}.`}`,
  "sentence.diagnosis.unexplained.cause.phase": (p: P) =>
    `${p.stuck === "true" ? `The rollout of ${p.service ?? ""} is stuck: the new replica ${p.pod ?? ""} is ${p.phase ?? ""}.` : `${p.pod ?? ""} is ${p.phase ?? ""}.`}`,
  "sentence.diagnosis.unexplained.cause.none": (p: P) =>
    `${p.stuck === "true" ? `The rollout of ${p.service ?? ""} is stuck.` : `${!p.reason || p.reason === "none" ? `${p.service ?? ""} is failing.` : `${p.service ?? ""} is failing (${p.reason ?? ""}).`}`}`,
  "sentence.diagnosis.unexplained.why.event": (p: P) =>
    `The newest event about it: ${p.event ?? ""}, “${p.note ?? ""}”.${!p.serving || p.serving === "none" ? `` : p.serving === "0" ? `` : p.serving === "1" ? ` The replica of the previous release still serves traffic.` : ` ${p.serving ?? ""} replicas of the previous release still serve traffic.`}`,
  "sentence.diagnosis.unexplained.why.noEvent": (p: P) =>
    `The cluster has recorded no event about it.${!p.serving || p.serving === "none" ? `` : p.serving === "0" ? `` : p.serving === "1" ? ` The replica of the previous release still serves traffic.` : ` ${p.serving ?? ""} replicas of the previous release still serve traffic.`}`,
  "sentence.diagnosis.unexplained.fix.waiting": (p: P) =>
    `Open ${p.pod ?? ""}'s events: the kubelet says there what it waits for.`,
  "sentence.diagnosis.unexplained.fix.terminated": (p: P) =>
    `Read ${p.pod ?? ""}'s logs and events for why it stopped.`,
  "sentence.diagnosis.unexplained.fix.pending": (p: P) =>
    `Open ${p.pod ?? ""}'s events: the scheduler and the kubelet say there why it hasn't started.`,
  "sentence.diagnosis.unexplained.fix.notReady": (p: P) =>
    `Make sure the app answers where its traffic check looks, and read ${p.pod ?? ""}'s logs.`,
  "sentence.diagnosis.unexplained.fix.phase": (p: P) => `Open ${p.pod ?? ""}'s events and logs.`,
  "sentence.diagnosis.unexplained.fix.none": (p: P) => `Open ${p.service ?? ""}'s events.`,
  "sentence.diagnosis.crashLoop.cause.exitCode": (p: P) =>
    `${p.container ?? ""} keeps crashing and restarting.`,
  "sentence.diagnosis.crashLoop.cause.exitsZero": (p: P) =>
    `${p.container ?? ""} keeps ending without an error, and restarting.`,
  "sentence.diagnosis.crashLoop.cause.notExecutable": (p: P) =>
    `${p.container ?? ""} can't run its command: the file isn't executable.`,
  "sentence.diagnosis.crashLoop.cause.commandNotFound": (p: P) =>
    `${p.container ?? ""} can't run its command: it isn't in the image.`,
  "sentence.diagnosis.crashLoop.cause.killed": (p: P) =>
    `${p.container ?? ""} keeps being killed, and restarting.`,
  "sentence.diagnosis.crashLoop.cause.terminated": (p: P) =>
    `${p.container ?? ""} keeps being stopped, and restarting.`,
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
  "sentence.diagnosis.runFailed.fix.logs": () => "The error is in the logs of the run that failed.",
  "sentence.diagnosis.runFailed.cause.exitCode": (p: P) =>
    `${p.container ?? ""}'s last run failed.`,
  "sentence.diagnosis.runFailed.cause.exitsZero": (p: P) =>
    `${p.container ?? ""}'s last run ended unexpectedly.`,
  "sentence.diagnosis.runFailed.cause.notExecutable": (p: P) =>
    `${p.container ?? ""}'s last run couldn't start its command: the file isn't executable.`,
  "sentence.diagnosis.runFailed.cause.commandNotFound": (p: P) =>
    `${p.container ?? ""}'s last run couldn't find its command in the image.`,
  "sentence.diagnosis.runFailed.cause.killed": (p: P) =>
    `${p.container ?? ""}'s last run was killed.`,
  "sentence.diagnosis.runFailed.cause.terminated": (p: P) =>
    `${p.container ?? ""}'s last run was stopped.`,
  "sentence.diagnosis.outOfMemory.cause.limit": (p: P) =>
    `${p.container ?? ""} ran out of memory and was stopped.`,
  "sentence.diagnosis.outOfMemory.cause.noLimit": (p: P) =>
    `${p.container ?? ""} was stopped because the machine ran short of memory.`,
  "sentence.diagnosis.outOfMemory.fix.limit": () =>
    "Raise the memory maximum, or find what uses the memory.",
  "sentence.diagnosis.outOfMemory.fix.noLimit": () =>
    "Set how much memory it reserves, and its maximum, to fit what it uses.",
  "sentence.diagnosis.imagePull.cause.notFound": (p: P) =>
    `${p.container ?? ""} can't download its image ${p.image ?? ""}: the registry has no such image.`,
  "sentence.diagnosis.imagePull.cause.unauthorized": (p: P) =>
    `${p.container ?? ""} can't download its image: the registry refused the login.`,
  "sentence.diagnosis.imagePull.cause.rateLimited": (p: P) =>
    `${p.container ?? ""} can't download its image: the registry limits how often it may.`,
  "sentence.diagnosis.imagePull.cause.unreachable": (p: P) =>
    `${p.container ?? ""} can't download its image: the registry can't be reached.`,
  "sentence.diagnosis.imagePull.cause.invalidName": (p: P) =>
    `${p.container ?? ""}'s image name ${p.image ?? ""} is wrong.`,
  "sentence.diagnosis.imagePull.cause.unknown": (p: P) =>
    `${p.container ?? ""} can't download its image ${p.image ?? ""}.`,
  "sentence.diagnosis.imagePull.fix.notFound": () => "Check the image's name and version tag.",
  "sentence.diagnosis.imagePull.fix.unauthorized": () =>
    "Give the project a login for the registry (an imagePullSecret), or fix the one it has.",
  "sentence.diagnosis.imagePull.fix.rateLimited": () => "Sign in to the registry, or use a mirror.",
  "sentence.diagnosis.imagePull.fix.unreachable": () => "Check the machine's network and DNS.",
  "sentence.diagnosis.imagePull.fix.invalidName": () => "Fix the image name.",
  "sentence.diagnosis.imagePull.fix.unknown": () =>
    "The replica's events have the registry's answer.",
  "sentence.diagnosis.probeFailure.cause.liveness": (p: P) =>
    `${p.container ?? ""}'s restart check failed ${p.count ?? ""} times${!p.output || p.output === "none" ? `.` : `: ${p.output ?? ""}`}`,
  "sentence.diagnosis.probeFailure.cause.readiness": (p: P) =>
    `${p.container ?? ""}'s traffic check failed ${p.count ?? ""} times${!p.output || p.output === "none" ? `.` : `: ${p.output ?? ""}`}`,
  "sentence.diagnosis.probeFailure.cause.startup": (p: P) =>
    `${p.container ?? ""} didn't finish starting in time.`,
  "sentence.diagnosis.probeFailure.fix.liveness": () =>
    "Check the restart check's path, port, and timing in Health checks.",
  "sentence.diagnosis.probeFailure.fix.readiness": () =>
    "Check the traffic check's path, port, and timing in Health checks.",
  "sentence.diagnosis.probeFailure.fix.startup": () =>
    "If it only starts slowly, allow more time in Health checks; if it should be up by now, its logs say why.",
  "sentence.gap.resourceLimits.none": (p: P) =>
    `${p.container ?? ""} sets no CPU or memory requests or limits.`,
  "sentence.gap.resourceLimits.partial": (p: P) =>
    `${p.container ?? ""} doesn't set ${words(items(p.missing))}.`,
  "sentence.gap.healthChecks.none": (p: P) =>
    `${p.container ?? ""} has no traffic check; traffic can reach it before it's ready.`,
  "sentence.gap.healthChecks.livenessOnly": (p: P) =>
    `${p.container ?? ""} has a restart check but no traffic check.`,
  "sentence.gap.ingressToHttpRoute.retired": (p: P) =>
    `Ingress ${p.ingress ?? ""} uses ${p.class ?? ""}, whose controller is no longer maintained.`,
  "sentence.gap.projectWithoutNetworkPolicy": (p: P) =>
    `Project ${p.namespace ?? ""} has no NetworkPolicy ${p.policy ?? ""}, so any replica in the cluster can reach its services.`,
  "sentence.gap.retiredIngressController": (p: P) =>
    `IngressClass ${p.class ?? ""} is served by ${p.controller ?? ""}, which is no longer maintained.`,
  "sentence.gap.danglingRoute.noService": (p: P) =>
    `${p.kind ?? ""} ${p.route ?? ""} sends traffic to ${items(p.services).length === 1 ? `Service` : `Services`} ${words(items(p.services))}, which ${items(p.services).length === 1 ? `doesn't` : `don't`} exist in this project.`,
  "sentence.gap.danglingRoute.noGateway": (p: P) =>
    `HTTPRoute ${p.route ?? ""} attaches to ${items(p.gateways).length === 1 ? `Gateway` : `Gateways`} ${words(items(p.gateways))}, which ${items(p.gateways).length === 1 ? `doesn't` : `don't`} exist.`,
  "sentence.gap.danglingRoute.noParent": (p: P) =>
    `HTTPRoute ${p.route ?? ""} names no Gateway, so it receives no traffic.`,
  "sentence.gap.danglingRoute.notAccepted": (p: P) =>
    `No Gateway has accepted HTTPRoute ${p.route ?? ""}${!p.reason || p.reason === "none" ? `` : ` (${p.reason ?? ""})`}, so it receives no traffic.`,
  "sentence.gap.blockedConnection": (p: P) =>
    `${p.at === "egress" ? `${p.from ?? ""}'s variables point to ${p.to ?? ""}, but ${p.from ?? ""}'s own NetworkPolicy blocks the outgoing connection.` : p.at === "both" ? `${p.from ?? ""}'s variables point to ${p.to ?? ""}, but NetworkPolicy blocks the connection at both ends.` : `${p.from ?? ""}'s variables point to ${p.to ?? ""}, but ${p.to ?? ""}'s NetworkPolicy doesn't let ${p.from ?? ""} in.`}`,
  "sentence.gap.noNetworkPolicy": (p: P) =>
    `No NetworkPolicy protects ${words(items(p.services))}: any replica in the cluster can reach ${items(p.services).length === 1 ? `it` : `them`}. Policies only work if the cluster's network plugin enforces them.${
      !p.cuts || p.cuts === "none"
        ? ``
        : ` Isolating also blocks ${words(
            items(p.cuts).flatMap((item) =>
              item === "otherNamespaces"
                ? [`traffic from other namespaces`]
                : item === "outside"
                  ? [`traffic out of the cluster`]
                  : item === "published"
                    ? [`NodePort and LoadBalancer traffic`]
                    : item === "connections"
                      ? [`a connection between its services`]
                      : [],
            ),
            true,
          )}.`
    }`,
  "sentence.gap.volumeWithoutBackup": (p: P) => `Volume ${p.claim ?? ""} has no backup schedule.`,
  "sentence.gap.databaseWithoutBackup": (p: P) =>
    `Database ${p.database ?? ""} has no backup schedule.`,
  "sentence.incident.componentDown.cause.ingressController": (p: P) =>
    `The ingress controller ${p.workload ?? ""} is down: none of its replicas is ready.`,
  "sentence.incident.componentDown.cause.certManager": (p: P) =>
    `cert-manager is down: no replica of ${p.workload ?? ""} is ready.`,
  "sentence.incident.componentDown.cause.clusterDns": (p: P) =>
    `Cluster DNS is down: no replica of ${p.workload ?? ""} is ready.`,
  "sentence.incident.componentDown.cause.metricsServer": (p: P) =>
    `metrics-server is down: no replica of ${p.workload ?? ""} is ready.`,
  "sentence.incident.componentDown.why.ingressController": (p: P) =>
    `The ingress controller brings requests for public domains to the services they route to; while it is down, ${Number(p.domains) === 1 ? `${p.domains ?? ""} domain` : `${p.domains ?? ""} domains`} and ${Number(p.services) === 1 ? `${p.services ?? ""} service` : `${p.services ?? ""} services`} get no requests from outside the cluster.`,
  "sentence.incident.componentDown.why.certManager": (p: P) =>
    `cert-manager issues and renews the certificates domains ask it for; while it is down, the certificates of ${Number(p.domains) === 1 ? `${p.domains ?? ""} domain` : `${p.domains ?? ""} domains`} are neither issued nor renewed, and browsers refuse a certificate once it expires.`,
  "sentence.incident.componentDown.why.clusterDns": (p: P) =>
    `Services find each other by name through cluster DNS; while it is down, ${Number(p.services) === 1 ? `${p.services ?? ""} service that calls` : `${p.services ?? ""} services that call`} another by name can't reach it.`,
  "sentence.incident.componentDown.why.metricsServer": () =>
    "metrics-server measures what each replica uses; while it is down, usage figures are missing and autoscaling stops.",
  "sentence.incident.componentDown.fix.ingressController": (p: P) =>
    `Check why ${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : ` in ${p.namespace ?? ""}`} has no ready replica; its events and logs say why.`,
  "sentence.incident.componentDown.fix.certManager": (p: P) =>
    `Check why ${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : ` in ${p.namespace ?? ""}`} has no ready replica; its events and logs say why.`,
  "sentence.incident.componentDown.fix.clusterDns": (p: P) =>
    `Check why ${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : ` in ${p.namespace ?? ""}`} has no ready replica; its events and logs say why.`,
  "sentence.incident.componentDown.fix.metricsServer": (p: P) =>
    `Check why ${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : ` in ${p.namespace ?? ""}`} has no ready replica; its events and logs say why.`,
  "sentence.incident.nodeNotReady.cause": (p: P) =>
    `${p.status === "Unknown" ? `Node ${p.node ?? ""} stopped reporting to the cluster` : `Node ${p.node ?? ""} is not ready`}${!p.reason || p.reason === "none" ? `` : ` (${p.reason ?? ""})`}.`,
  "sentence.incident.nodeNotReady.why": (p: P) =>
    `A node that isn't ready starts nothing new, and after a grace period, 5 minutes by default, the cluster removes its replicas so they can start elsewhere. ${Number(p.pods) === 1 ? `${p.pods ?? ""} replica runs` : `${p.pods ?? ""} replicas run`} on it, from ${Number(p.services) === 1 ? `${p.services ?? ""} service` : `${p.services ?? ""} services`}.`,
  "sentence.incident.nodeNotReady.fix": (p: P) =>
    `Check the machine behind ${p.node ?? ""}: whether it is on, its network, and its node agent (kubelet). If it won't come back, drain it and replace it.`,
  "sentence.trace.dns.ok": (p: P) => `${p.host ?? ""} resolves.`,
  "sentence.trace.dns.notFound": (p: P) =>
    `${p.host ?? ""} doesn't resolve from inside the cluster; DNS elsewhere may still answer, so Zeabur can't tell.`,
  "sentence.trace.dns.unknown": (p: P) =>
    `${p.host ?? ""} wasn't looked up, or the lookup didn't answer in time.`,
  "sentence.trace.address.noStatus": (p: P) =>
    `${p.route ?? ""} has no address in its status yet, so the domain's address can't be compared with it.`,
  "sentence.trace.address.unresolved": (p: P) =>
    `The domain didn't resolve, so its address can't be compared with ${p.route ?? ""}'s.`,
  "sentence.trace.address.ok": (p: P) =>
    `The domain points to the address ${p.route ?? ""} is served on.`,
  "sentence.trace.address.elsewhere": (p: P) =>
    `The domain points to another address than ${p.route ?? ""}'s; a CDN or proxy in front of the cluster answers with its own.`,
  "sentence.trace.controller.classesUnreadable": (p: P) =>
    `Zeabur can't read the cluster's IngressClasses, so it can't tell which controller serves ${p.route ?? ""}.`,
  "sentence.trace.controller.classMissing": (p: P) =>
    `${p.route ?? ""} asks for IngressClass ${p.class ?? ""}, which the cluster doesn't have.`,
  "sentence.trace.controller.unreadable": (p: P) =>
    `Zeabur can't read the cluster's components, so it can't tell whether the ingress controller of ${p.route ?? ""} runs.`,
  "sentence.trace.controller.notRecognised": (p: P) =>
    `Zeabur doesn't recognize the ingress controller${!p.controller || p.controller === "none" ? `` : ` ${p.controller ?? ""}`} that serves ${p.route ?? ""}, so it can't tell whether it runs.`,
  "sentence.trace.controller.ok": (p: P) =>
    `The ingress controller ${p.workload ?? ""} has a ready replica.`,
  "sentence.trace.controller.down": (p: P) =>
    `The ingress controller ${p.workload ?? ""} has no ready replica, so nothing serves ${p.route ?? ""}.`,
  "sentence.trace.certificate.defaultCertificate": (p: P) =>
    `${p.route ?? ""} lists ${p.host ?? ""} for HTTPS without a secret, so the controller serves its default certificate.`,
  "sentence.trace.certificate.ok": (p: P) =>
    `The certificate for ${p.host ?? ""} is issued${!p.until || p.until === "none" ? `` : `, valid until ${p.until ?? ""}`}.`,
  "sentence.trace.certificate.notReady": (p: P) =>
    `Certificate ${p.certificate ?? ""} for ${p.host ?? ""} isn't issued${!p.reason || p.reason === "none" ? `` : ` (${p.reason ?? ""})`}.`,
  "sentence.trace.certificate.issuing": (p: P) =>
    `cert-manager is issuing certificate ${p.certificate ?? ""} for ${p.host ?? ""}.`,
  "sentence.trace.certificate.pending": (p: P) =>
    `Certificate ${p.certificate ?? ""} for ${p.host ?? ""} has no result yet.`,
  "sentence.trace.certificate.unreadable": (p: P) =>
    `Zeabur can't read the project's secrets, so it can't check the certificate for ${p.host ?? ""} in secret ${p.secret ?? ""}.`,
  "sentence.trace.certificate.notChecked": (p: P) =>
    `Secret ${p.secret ?? ""} holds the certificate for ${p.host ?? ""}; Zeabur doesn't check it further.`,
  "sentence.trace.certificate.secretMissing": (p: P) =>
    `${p.route ?? ""} reads the certificate for ${p.host ?? ""} from secret ${p.secret ?? ""}, which isn't in the project; the controller may serve a default certificate that Zeabur can't check.`,
  "sentence.trace.route.noStatus": (p: P) =>
    `Nothing has written ${p.route ?? ""}'s status yet, so Zeabur can't tell whether it's in effect.`,
  "sentence.trace.route.ok": (p: P) =>
    `${p.route ?? ""} is in effect: its controller wrote an address into its status.`,
  "sentence.trace.route.accepted": (p: P) => `A Gateway accepted ${p.route ?? ""}.`,
  "sentence.trace.route.notAccepted": (p: P) =>
    `${p.route ?? ""}'s entry point reports ${p.condition ?? ""} ${p.reason ?? ""}; requests don't go through.`,
  "sentence.trace.service.unreadable": (p: P) =>
    `Zeabur can't read the project's private addresses (Services), so it can't check ${p.endpoint ?? ""}.`,
  "sentence.trace.service.missing": (p: P) =>
    `No private address (Service) is named ${p.endpoint ?? ""}.`,
  "sentence.trace.service.portMissing": (p: P) =>
    `Private address ${p.endpoint ?? ""} has no port ${p.port ?? ""}.`,
  "sentence.trace.service.selectsOther": (p: P) =>
    `Private address ${p.endpoint ?? ""} sends to other replicas, not ${p.service ?? ""}'s.`,
  "sentence.trace.service.ok": (p: P) =>
    `Private address ${p.endpoint ?? ""} sends to ${p.service ?? ""}.`,
  "sentence.trace.targetPort.unknown": (p: P) =>
    `Zeabur couldn't read the containers, so the port ${p.endpoint ?? ""} sends to isn't checked.`,
  "sentence.trace.targetPort.notReached": (p: P) =>
    `Not checked: traffic stops before it reaches a port of ${p.endpoint ?? ""}.`,
  "sentence.trace.targetPort.mismatch": (p: P) =>
    `${p.endpoint ?? ""} sends to port ${p.targetPort ?? ""}, which the containers don't declare${!p.ports || p.ports === "none" ? `` : ` (they declare ${p.ports ?? ""})`}.`,
  "sentence.trace.targetPort.undeclared": (p: P) =>
    `The containers declare no ports, so whether they listen where ${p.endpoint ?? ""} sends is unknown.`,
  "sentence.trace.targetPort.ok": (p: P) =>
    `${p.endpoint ?? ""} sends to a port the containers declare.`,
  "sentence.trace.ready.scaledToZero": (p: P) => `${p.service ?? ""} is scaled to 0 replicas.`,
  "sentence.trace.ready.ok": (p: P) =>
    `${p.service ?? ""} has ${p.ready ?? ""}/${p.pods ?? ""} replicas ready.`,
  "sentence.trace.ready.starting": (p: P) =>
    `${p.service ?? ""} is starting; no replica is ready yet.`,
  "sentence.trace.ready.noneReady": (p: P) => `No replica of ${p.service ?? ""} is ready.`,
  "sentence.diagnosis.unreachable.cause.ingressClassMissing": (p: P) =>
    `Requests to the domain of ${p.route ?? ""} don't reach ${p.service ?? ""}: it asks for IngressClass ${p.class ?? ""}, which the cluster doesn't have.`,
  "sentence.diagnosis.unreachable.why.ingressClassMissing": () =>
    "An ingress controller serves only the Ingresses of its own class; with no such class, none serves this one.",
  "sentence.diagnosis.unreachable.fix.ingressClassMissing": (p: P) =>
    `Set ${p.route ?? ""}'s class to one the cluster has, or install the controller for ${p.class ?? ""}.`,
  "sentence.diagnosis.unreachable.cause.controllerDown": (p: P) =>
    `Requests to the domain of ${p.route ?? ""} don't reach ${p.service ?? ""}: the ingress controller ${p.workload ?? ""} has no ready replica.`,
  "sentence.diagnosis.unreachable.why.controllerDown": () =>
    "The ingress controller brings requests for a domain into the cluster; while none of its replicas is ready, nothing answers them.",
  "sentence.diagnosis.unreachable.fix.controllerDown": (p: P) =>
    `Check why ${p.workload ?? ""}${!p.namespace || p.namespace === "none" ? `` : ` in ${p.namespace ?? ""}`} has no ready replica; its events and logs say why.`,
  "sentence.diagnosis.unreachable.cause.certificateNotReady": (p: P) =>
    `HTTPS to ${p.host ?? ""} fails: certificate ${p.certificate ?? ""} isn't issued${!p.reason || p.reason === "none" ? `` : ` (${p.reason ?? ""})`}.`,
  "sentence.diagnosis.unreachable.why.certificateNotReady": (p: P) =>
    `cert-manager writes the certificate into secret ${p.secret ?? ""} once it is issued; until then the controller serves its default certificate, which browsers refuse.`,
  "sentence.diagnosis.unreachable.fix.certificateNotReady": (p: P) =>
    `Certificate ${p.certificate ?? ""}'s status and events say why it isn't issued.`,
  "sentence.diagnosis.unreachable.cause.backendPortMissing": (p: P) =>
    `A route to ${p.service ?? ""} names port ${p.port ?? ""}, which its private address ${p.endpoint ?? ""} doesn't have.`,
  "sentence.diagnosis.unreachable.why.backendPortMissing": () =>
    "A route sends to one port of a private address, by number or name; a port the address doesn't have receives nothing.",
  "sentence.diagnosis.unreachable.fix.backendPortMissing": (p: P) =>
    `Point the route at a port ${p.endpoint ?? ""} has, or add port ${p.port ?? ""} to ${p.endpoint ?? ""}.`,
  "sentence.diagnosis.unreachable.cause.selectsOther": (p: P) =>
    `A route to ${p.service ?? ""} sends to ${p.endpoint ?? ""}, which sends to other replicas, not ${p.service ?? ""}'s.`,
  "sentence.diagnosis.unreachable.why.selectsOther": () =>
    "A private address sends to the replicas whose labels match its selector.",
  "sentence.diagnosis.unreachable.fix.selectsOther": (p: P) =>
    `Make ${p.endpoint ?? ""}'s selector match ${p.service ?? ""}'s labels, or point the route at ${p.service ?? ""}'s own private address.`,
  "sentence.diagnosis.unreachable.cause.scaledToZero": (p: P) =>
    `Nothing reaches ${p.service ?? ""}: it is scaled to 0 replicas.`,
  "sentence.diagnosis.unreachable.why.scaledToZero": () =>
    "A private address sends only to running replicas, and none is set to run.",
  "sentence.diagnosis.unreachable.fix.scaledToZero": (p: P) =>
    `Scale ${p.service ?? ""} to at least 1 replica.`,
  "sentence.gap.ephemeralData.none": (p: P) =>
    `${p.container ?? ""} keeps its ${p.software ?? ""} data in ${p.path ?? ""}, which is on no volume; it's lost whenever the replica is replaced.`,
  "sentence.gap.ephemeralData.ephemeral": (p: P) =>
    `${p.container ?? ""} keeps its ${p.software ?? ""} data in ${p.path ?? ""}, on a volume that lasts only as long as the replica; it's lost whenever the replica is replaced.`,
}
