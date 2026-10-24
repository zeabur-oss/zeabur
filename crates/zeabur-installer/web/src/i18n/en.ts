// English: the source of every message key. The other locales are typed as `Messages`, so a key
// missing there (or added there only) is a TypeScript error.

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

const mib = (value: number | string) => `${Number(value).toLocaleString("en")} MiB`

export const en = {
  "app.name": "Installer",
  "app.language": "Language",
  "app.theme": "Theme",
  "theme.system": "System theme",
  "theme.light": "Light",
  "theme.dark": "Dark",
  "common.sep": ", ",
  "common.colon": ": ",
  "common.list": (items: string[]) => new Intl.ListFormat("en").format(items),
  "common.sentences": (parts: string[]) => parts.filter(Boolean).join(" "),
  "common.back": "Back",
  "common.continue": "Continue",
  "common.retry": "Try again",
  "common.loading": "Loading…",
  "common.copy": "Copy",
  "common.download": "Download",
  "common.newTab": "(opens in a new tab)",
  "common.error": "Something went wrong",
  "common.gib": (gib: string) => `${gib} GiB`,
  "common.mib": (mib: number) => `${mib.toLocaleString("en")} MiB`,

  "step.host": "Server",
  "step.access": "Access",
  "step.components": "Components",
  "step.review": "Review",
  "step.install": "Install",
  "step.of": (n: number, total: number) => `Step ${n} of ${total}`,

  // Server
  "host.title": "Check the server",
  "host.description":
    "Nothing changes until you install. A failed check has to be fixed first; a warning is worth reading.",
  "host.server": "This server",
  "host.name": "Host name",
  "host.arch": "Architecture",
  "host.memory": "Memory",
  "host.addresses": "Addresses",
  "host.checks": "Checks",
  "host.recheck": "Check again",
  "host.passed": "Ready to install",
  "host.warned": (n: number) => `Ready, with ${plural(n, "warning", "warnings")}`,
  "host.failed": (n: number) => `${plural(n, "check", "checks")} to fix`,
  "host.catalog": "The component catalog could not be read",
  "host.configFrom": (path: string) => `Starting from ${path}.`,
  "host.configNew": (path: string) => `Your choices will be saved to ${path}.`,
  "host.passedCount": (n: number) => `${plural(n, "check", "checks")} passed`,

  "check.root": "Administrator",
  "check.systemd": "systemd",
  "check.time": "Clock",
  "check.swap": "Swap",
  "check.other Kubernetes": "Other Kubernetes",
  "check.ports": "Ports",
  "check.disk space": "Disk space",
  "check.disk encryption": "Disk encryption",

  "code.root.ok": () => "Running as root.",
  "code.root.no": () => "Start the installer with sudo.",
  "code.systemd.ok": () => "systemd manages services on this server.",
  "code.systemd.no": () => "RKE2 runs as a systemd service, and this server does not use systemd.",
  "code.time.ok": () => "The clock is synchronised.",
  "code.time.drift": () =>
    "The clock is not synchronised; certificates and sign-ins fail when it drifts. Run timedatectl set-ntp true.",
  "code.time.unknown": () =>
    "Could not tell whether the clock is synchronised. Run timedatectl and check that the system clock is synchronised.",
  "code.swap.off": () => "Swap is off.",
  "code.swap.on": () =>
    "Swap is on; the kubelet expects it off. Run swapoff -a and remove the swap entry from /etc/fstab.",
  "code.swap.unknown": () => "Could not tell whether swap is on. Run swapon --show to see it.",
  "code.others.none": () => "None found.",
  "code.others.found": (found: string) =>
    `Found ${found}. Uninstall it first: two distributions fight over the same ports.`,
  "code.ports.free": () => "80, 443, 6443, 9345 and 10250 are free.",
  "code.ports.busy": (busy: string) => `In use: ${busy}. Run ss -ltnp to see what holds them.`,
  "code.disk.unknown": (path: string) =>
    `Could not read the free space under ${path}. Run df -h ${path} to see it.`,
  "code.disk.low": (gib: string, path: string, need: string) =>
    `Only ${gib} GiB free under ${path}; RKE2 needs at least ${need} GiB.`,
  "code.disk.tight": (gib: string, path: string, need: string) =>
    `${gib} GiB free under ${path}; ${need} GiB or more leaves room for logs, metrics and apps.`,
  "code.disk.ok": (gib: string, path: string) => `${gib} GiB free under ${path}.`,
  "code.crypt.yes": (path: string) => `${path} is on an encrypted device.`,
  "code.crypt.requiredPlain": (path: string) =>
    `${path} is not on an encrypted device, and the configuration requires one.`,
  "code.crypt.requiredUnknown": (path: string) =>
    `Could not tell whether ${path} is encrypted, and the configuration requires it.`,
  "code.crypt.no": (path: string) =>
    `${path} is not on an encrypted device. Secrets are encrypted in the cluster; volumes are not. Fine for a test machine; for production data, install on an encrypted disk.`,
  "code.crypt.unknown": (path: string) =>
    `Could not tell whether ${path} is encrypted. Run lsblk to see whether a crypt device is under it.`,

  // Access
  "access.title": "How people reach Zeabur",
  "access.description":
    "Zeabur and its sign-in service each need a domain that points at this server.",
  "access.mode": "Certificates",
  "access.domain": "Public domain",
  "access.local": "Test machine",
  "access.domainHint":
    "Let's Encrypt issues the certificates, so ports 80 and 443 must be reachable from the internet.",
  "access.localHint":
    "A certificate authority made on this server signs them. Browsers trust it once you import it, which the last step offers.",
  "access.names": "Domains",
  "access.zeabur": "Zeabur",
  "access.zeaburHint": "Where people open Zeabur.",
  "access.identity": "Sign-in",
  "access.identityHint": "Zeabur Identity, where people sign in with a passkey.",
  "access.sslip": "Use sslip.io domains",
  "access.sslipHint": (address: string) =>
    `Names under ${address}.sslip.io resolve to ${address} with no DNS setup.`,
  "access.resolves": (addresses: string) => `Points at this server (${addresses}).`,
  "access.elsewhere": (addresses: string) =>
    `Points at ${addresses}, not an address of this server. Fine if that is its public address.`,
  "access.unresolved": "Does not resolve yet. Add a DNS record pointing at this server.",
  "access.checking": "Looking up…",
  "access.invalid": "Write a domain only, without https:// or a path.",
  "access.same": "Use two different domains.",

  // Components
  "components.title": "Choose components",
  "components.description":
    "Start from a preset, then add or remove components. What a component needs is added with it.",
  "components.preset": "Preset",
  "preset.minimal": "Minimal",
  "preset.standard": "Standard",
  "preset.secure": "Hardened",
  "presetHint.minimal": "Ingress, certificates and metrics-server: what Zeabur needs to run.",
  "presetHint.standard": "Everything the catalog recommends for one server.",
  "presetHint.secure":
    "Standard, plus runtime threat detection, vulnerability scanning and intrusion prevention.",
  "components.chosen": (n: number) => `${plural(n, "component", "components")} chosen`,
  "components.needed": "Needed by another",
  "components.privileged": "Privileged",
  "components.privilegedHint":
    "Runs with host access, so its namespace is exempt from the restricted Pod Security level.",
  "components.unsupported": "Not on RKE2",
  "components.runsOn": (where: string) => `Runs on ${where} only`,
  "components.unsupportedOn": (profile: string) => `Not supported on ${profile}`,
  "components.details": (name: string) => `About ${name}`,
  "components.customise": (n: number) => `Customise (${n})`,
  "components.settings": "Settings",
  "components.required": "Required",
  "components.generated": "Generated when left empty.",
  "components.list": "A list: [a, b].",
  "components.secret": "Saved to a file only root can read, never in the configuration.",
  "capability.certificates": "Certificates",
  "capability.config-reload": "Config reload",
  "capability.dns": "DNS",
  "capability.event-autoscaling": "Event autoscaling",
  "capability.ingress": "Ingress",
  "capability.intrusion-prevention": "Intrusion prevention",
  "capability.logs": "Logs",
  "capability.metrics": "Metrics",
  "capability.node-updates": "Node updates",
  "capability.observability-operator": "Monitoring operator",
  "capability.remote-access": "Remote access",
  "capability.resource-recommendations": "Resource recommendations",
  "capability.runtime-security": "Runtime security",
  "capability.uptime-probes": "Uptime probes",
  "capability.volume-backup": "Volume backups",
  "capability.vulnerability-scanning": "Vulnerability scanning",

  // Review
  "review.title": "Review",
  "review.description": (path: string) =>
    `This is what will be installed. The configuration is saved to ${path}, so the same install can be repeated from the command line.`,
  "review.summary": "Summary",
  "review.kubernetes": "Kubernetes",
  "review.rke2": (version: string) => `RKE2 ${version}`,
  "review.rke2Custom": (version: string) => `RKE2 ${version} (unsupported)`,
  "review.access": "Access",
  "review.components": "Components",
  "review.memory": "Memory at idle",
  "review.time": "Time",
  "review.timeValue": "10 to 20 minutes",
  "review.warnings": "Before you install",
  "review.plan": "Full plan",
  "review.config": "Configuration file",
  "review.details": "Details",
  "review.charts": "Charts and versions",
  "review.install": "Install",
  "review.blocked": "Fix the failed server checks first.",
  "review.blockedMemory": "The plan needs more memory than this server has.",
  "review.blockedTitle": (n: number) => `${plural(n, "server check", "server checks")} failed`,
  "review.blockedHint": "Fix these on the server, then check again.",
  "review.backToChecks": "Back to server checks",
  "review.leftOut": "Left out",
  "review.leftOutItem": (name: string, reason: string) => `${name} (${reason})`,
  "review.privileged": (n: number) => `Privileged: ${n}`,
  "review.fromPreset": (n: number) => `${n} from the preset`,
  "review.added": (n: number) => `${n} added by you`,
  "review.needed": (n: number) => `${n} needed by another`,
  "review.group.preset": "From the preset",
  "review.group.enabled": "Added by you",
  "review.group.dependency": "Needed by another",
  "review.group.leftOut": "Left out",
  "review.col.component": "Component",
  "review.col.chart": "Chart",
  "review.col.version": "Version",
  "review.col.memory": "Idle memory",
  "review.manifests": "Manifest",

  "memory.fits": "Fits",
  "memory.tight": "Tight",
  "memory.over": "Does not fit",
  "memory.usage": (used: number, host: number) =>
    `about ${used.toLocaleString("en")} of ${host.toLocaleString("en")} MiB`,
  "memory.overBy": (mib: number) => `Over by ${mib.toLocaleString("en")} MiB.`,
  "memory.tightBy": "Less than a fifth is left for your apps.",
  "memory.relief": (names: string, mib: number) =>
    `Leaving out ${names} frees ${mib.toLocaleString("en")} MiB.`,
  "memory.more": "Or give the server more memory.",
  "memory.add": "Give the server more memory.",
  "memory.unmeasured": (n: number) =>
    `${plural(n, "component has", "components have")} no measurement yet, so it may be more.`,

  "warning.kubernetes.untested": (version: string, tested: string) =>
    `The catalog was not tested on Kubernetes ${version}; it lists ${tested}.`,
  "warning.metrics.missing": () => "Without VictoriaMetrics, Zeabur shows no usage history.",
  "warning.local.dns": (identity: string, zeabur: string) =>
    `${zeabur} and ${identity} must resolve to this server through DNS, for browsers and for the server itself. A hosts file is not enough: pods do not read it.`,
  "warning.memory.low": (host: string) =>
    `RKE2 asks for a server with at least 4 GiB of memory; this server has ${mib(host)}.`,
  "warning.memory.tight": (idle: string, host: string) =>
    `At idle, RKE2 and the components use about ${mib(idle)} of this server's ${mib(host)}.`,
  "warning.rke2.unsupported": (version: string) =>
    `RKE2 ${version} is not supported by this installer: its downloads are checked only against the release's own checksums.`,
  "warning.join.sameRegion": () =>
    "Put every node at the same hosting provider and in the same region. Different availability zones in one region are fine. Between providers or regions, latency slows etcd, the Kubernetes API and storage, and can make the cluster unstable.",
  "warning.join.provider.differs": (here: string, server: string) =>
    `This VM is at ${here}, the server at ${server}. Nodes need the same hosting provider.`,
  "warning.join.region.differs": (here: string, server: string) =>
    `This VM is in ${here}, the server in ${server}. Nodes need the same region; a different availability zone is fine.`,
  "warning.join.location.unknown": () =>
    "Could not read the provider and region of this VM or the server from cloud metadata. Check yourself that they match.",
  "warning.join.rtt.high": (median: string, limit: string) =>
    `The server is ${median} ms away (median of 5 TCP connects to port 9345); within a region it is usually under ${limit} ms.`,
  "warning.join.rtt.critical": (median: string, limit: string) =>
    `The server is ${median} ms away (median of 5 TCP connects to port 9345). Above ${limit} ms the cluster will be slow and unstable; this looks like another region.`,
  "warning.join.port.closed": (ports: string, host: string) =>
    `TCP ${ports} on ${host} cannot be reached from this VM; check the firewall or security group.`,
  "warning.join.ports": () =>
    "Allow these between the nodes: TCP 9345 and 6443 to the servers, TCP 10250 and 9099 and UDP 8472 (Canal VXLAN) between all nodes, and TCP 2379 to 2381 between servers. Keep UDP 8472 closed to the internet.",
  "warning.join.udp": () =>
    "UDP 8472 cannot be checked from here. If it is blocked, pods on different nodes cannot reach each other.",
  "warning.join.servers.ha": () =>
    "The control plane survives losing a server only with three or more servers: etcd needs a majority. With two, losing either one stops the cluster, so add a third.",
  "warning.join.storage.local": () =>
    "Volumes on the default local-path storage class stay on the node that created them: a pod using one runs only on that node, and the data goes with the node.",
  "warning.join.token.unpinned": () =>
    "The token has no CA hash (K10<hash>::…), so this VM trusts whichever server answers at that address. Use the token exactly as node add printed it.",
  "warning.join.server.unreachable": (server: string) =>
    `Cannot connect to ${server}. Check the address and that TCP 9345 is open from this VM.`,

  // Install
  "install.title": "Installing",
  "install.description":
    "This takes 10 to 20 minutes. Closing this page does not stop it; open the link again to follow along.",
  "install.failedTitle": "The install stopped",
  "install.failed":
    "Install again starts from the top, skips what is already in place and removes nothing, including components you leave out now.",
  "install.again": "Install again",
  "install.log": "Log",
  "install.showLog": "Show the log",
  "install.hideLog": "Hide the log",
  "install.running": "In progress",
  "install.waiting": "Waiting",
  "install.done": "Done",
  "install.stopped": "Stopped here",
  "install.stoppedAt": (phase: string) => `Stopped at ${phase}`,
  "install.runningAt": (phase: string) => `Installing ${phase}`,
  "install.doneCount": (n: number) => `${n} done`,
  "install.waitingCount": (n: number) => `${n} waiting`,
  "install.foldDone": (n: number) => `${plural(n, "step", "steps")} done`,
  "install.foldWaiting": (n: number) => `${n} more waiting`,
  "install.showAll": (n: number) => `Show all ${n}`,
  "install.showFewer": "Show fewer",
  "install.cause.memory": (phase: string) =>
    `The server ran out of memory while installing ${phase}.`,
  "install.next.memory": "Leave out components or give the server more memory, then install again.",
  "install.cause.host": "A server check failed.",
  "install.next.host": "Fix it on the server, then install again.",
  "install.cause.timeout": (phase: string) => `${phase} did not become ready in time.`,
  "install.cause.other": (phase: string) => `The install stopped at ${phase}.`,
  "install.next.log": "The end of the log says what went wrong.",
  "install.toComponents": "Choose components",
  "phase.host": "Check the server",
  "phase.rke2": (version: string) => `Install Kubernetes (RKE2 ${version})`,
  "phase.namespaces": "Namespaces and network policies",
  "phase.storage": "Storage",
  "phase.ca": "Certificate authority",
  "phase.identity": "Zeabur Identity",
  "phase.zeabur": "Zeabur",
  "phase.done": "Finish",

  // Done
  "done.title": "Zeabur is ready",
  "done.description":
    "Register the first administrator's passkey, then sign in to Zeabur. Stop the installer with Ctrl-C when you are done.",
  "done.setup": "First administrator",
  "done.setupHint": "This link works once, within 24 hours.",
  "done.register": "Register a passkey",
  "done.open": "Open Zeabur",
  "done.zeabur": "Zeabur",
  "done.identity": "Sign-in",
  "done.ca": "Certificate authority",
  "done.caHint":
    "Import it into your browser or system and trust it for websites, or browsers will warn about these sites.",
  "done.caDownload": "Download certificate",
  "done.noLink":
    "Zeabur was already set up, so no new link was made. Run zeabur-identityctl setup-link --username admin for one.",
}

export type Messages = typeof en
