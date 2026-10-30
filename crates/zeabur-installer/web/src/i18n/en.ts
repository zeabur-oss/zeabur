// English: the source of every message key. The other locales are typed as `Messages`, so a key
// missing there (or added there only) is a TypeScript error.

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`

const mib = (value: number | string) => `${Number(value).toLocaleString("en")} MiB`

const list = (items: string[]) => new Intl.ListFormat("en").format(items)

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
  "use.title": "Who is it for",
  "use.personal": "Just me",
  "use.personalHint":
    "Uses the least. 1.5 GiB is enough. Only you: no databases, and you cannot invite anyone.",
  "use.team": "A team",
  "use.teamHint": "Needs 4 GiB. Accounts, members and databases.",
  "use.teamTooSmall":
    "Needs a server with 4 GiB of memory. This one has less, so only a personal install fits.",
  "dist.title": "Kubernetes",
  "dist.suggested": "Suggested",
  "dist.suggestK3s": (gib: string) =>
    `This server has ${gib} GiB of memory, so K3s is suggested: the most balanced. It uses less itself and leaves more for your apps.`,
  "dist.suggestRke2": (gib: string) =>
    `This server has ${gib} GiB of memory, so RKE2 is suggested: the easiest to grow. You can later run the control plane on three servers, so losing one does not stop it.`,
  "dist.k3sHint": "The most balanced. Uses less memory than RKE2, and you can add workers later.",
  "dist.rke2Hint":
    "The easiest to grow. Add workers, or run the control plane on three servers so losing one does not stop it. Best from 8 GiB.",
  "dist.tooSmall": "This server does not have enough memory for it.",
  "dist.rke2NotSuggested":
    "This server has less than 8 GiB. RKE2 itself uses more memory than K3s, so unless you plan to add servers, K3s fits better.",
  "host.title": "Check the server",
  "host.description":
    "Nothing changes until you install. A failed check has to be fixed first; a warning is worth reading.",
  "host.server": "This server",
  "host.name": "Host name",
  "host.nodeName": "Node name",
  "host.nodeNameHint":
    "Kubernetes knows this server by this name. It stays the same if the host name changes later.",
  "host.nodeNameKept": "This server is already installed under this name, so it stays.",
  "host.arch": "Architecture",
  "host.memory": "Memory",
  "host.addresses": "Addresses",
  "host.checks": "Checks",
  "host.recheck": "Check again",
  "host.passed": "Ready to install",
  "host.warned": (n: number) => `Ready, with ${plural(n, "warning", "warnings")}`,
  "host.failed": (n: number) => `${plural(n, "check", "checks")} to fix`,
  "host.catalog": "Unable to read the component catalog",
  "host.configFrom": (path: string) => `Using the settings in ${path}.`,
  "host.configNew": (path: string) => `Your choices will be saved to ${path}.`,
  "host.passedCount": (n: number) => `${plural(n, "check", "checks")} passed`,

  "check.root": "Administrator",
  "check.systemd": "systemd",
  "check.time": "Clock",
  "check.swap": "Swap",
  "check.crashKernel": "Crash kernel",
  "check.otherKubernetes": "Other Kubernetes",
  "check.ports": "Ports",
  "check.diskSpace": "Disk space",
  "check.diskEncryption": "Disk encryption",
  "check.nodePackages": "Packages",
  "check.kernelModules": "Kernel modules",
  "check.services": "Services",
  "check.multipathd": "multipathd",
  "check.docker": "Docker",

  "code.root.ok": () => "Running as root.",
  "code.root.no": () => "Start the installer with sudo.",
  "code.systemd.ok": () => "systemd manages services on this server.",
  "code.systemd.no": () =>
    "Kubernetes runs as a systemd service, and this server does not use systemd.",
  "code.time.ok": () => "The clock is synchronized.",
  "code.time.drift": () =>
    "The clock is not synchronized; certificates and sign-ins fail when it drifts. Run timedatectl set-ntp true.",
  "code.time.unknown": () =>
    "Could not tell whether the clock is synchronized. Run timedatectl and check that the system clock is synchronized.",
  "code.swap.off": () => "Swap is off.",
  "code.swap.on": () =>
    "Swap is on; the kubelet expects it off. Run swapoff -a and remove the swap entry from /etc/fstab.",
  "code.swap.unknown": () => "Could not tell whether swap is on. Run swapon --show to see it.",
  "code.crash.none": () => "No memory is kept for a crash kernel.",
  "code.crash.kept": (mib: string) =>
    `The kernel keeps ${mib} MiB for a crash kernel (kdump). If you do not need it, run sudo apt purge kdump-tools && sudo update-grub, then reboot to give the memory back.`,
  "code.others.none": () => "None found.",
  "code.others.found": (...paths: string[]) =>
    `Found files of another Kubernetes distribution (${list(paths)}). If it still runs, uninstall it first.`,
  "code.ports.free": (...ports: string[]) => `${list(ports)} are free.`,
  "code.ports.busy": (...ports: string[]) =>
    `In use: ${list(ports)}. Run ss -ltnp to see what holds them.`,
  "code.disk.unknown": (path: string) =>
    `Could not read the free space under ${path}. Run df -h ${path} to see it.`,
  "code.disk.low": (gib: string, path: string, need: string) =>
    `Only ${gib} GiB free under ${path}; Kubernetes needs at least ${need} GiB.`,
  "code.disk.tight": (gib: string, path: string, need: string) =>
    `${gib} GiB free under ${path}; ${need} GiB or more leaves room for logs, metrics and apps.`,
  "code.disk.ok": (gib: string, path: string) => `${gib} GiB free under ${path}.`,
  "code.crypt.yes": (path: string) => `${path} is on an encrypted device.`,
  "code.crypt.requiredPlain": (path: string) =>
    `${path} is not on an encrypted device, and the configuration requires one.`,
  "code.crypt.requiredUnknown": (path: string) =>
    `Could not tell whether ${path} is encrypted, and the configuration requires it.`,
  "code.crypt.no": (path: string) =>
    `${path} isn't on an encrypted disk, so volume and database files are stored unencrypted. Most VPS plans can't encrypt it afterwards, so you can go ahead. For sensitive data, pick a plan whose provider encrypts disks; this check can't see that.`,
  "code.crypt.unknown": (path: string) =>
    `Could not tell whether ${path} is encrypted. Run lsblk to see whether a crypt device is under it.`,
  "code.docker.none": () => "Docker is not on this server.",
  "code.docker.found": (...found: string[]) =>
    `Docker is on this server (${list(found)}). It blocks Pods from reaching the internet, so certificates fail. Stop and remove Docker, then check again.`,
  "code.docker.forwardDrop": () =>
    "The firewall drops forwarded traffic, so Pods cannot reach the internet and certificates fail. Docker or ufw usually sets this; remove or change it, then check again.",
  "code.prep.package.ok": (binary: string) => `${binary} is installed.`,
  "code.prep.package.toInstall": (binary: string, component: string) =>
    `${component} needs ${binary}. The install adds it.`,
  "code.prep.package.missing": (binary: string, component: string) =>
    `${component} needs ${binary}, and it is not installed. Run zeabur-installer apply to add it.`,
  "code.prep.module.ok": (module: string) => `The ${module} kernel module is loaded.`,
  "code.prep.module.toLoad": (module: string, component: string) =>
    `${component} needs the ${module} kernel module. The install loads it.`,
  "code.prep.module.missing": (module: string, component: string) =>
    `${component} needs the ${module} kernel module, and it is not loaded. Run zeabur-installer apply to load it.`,
  "code.prep.service.ok": (service: string) => `${service} is running.`,
  "code.prep.service.toStart": (service: string, component: string) =>
    `${component} needs ${service} running. The install starts it.`,
  "code.prep.service.stopped": (service: string, component: string) =>
    `${component} needs ${service} running, and it is stopped. Run zeabur-installer apply to start it.`,
  "code.prep.multipath.toBlock": (component: string, file: string) =>
    `multipathd would take over ${component}'s disks. The install writes ${file} to keep it off them.`,
  "code.prep.multipath.open": (component: string, file: string) =>
    `multipathd can take over ${component}'s disks while ${file} is missing. Run zeabur-installer apply to write it.`,

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
    "A certificate authority made on this server signs them, for Zeabur, sign-in and your apps' domains. Browsers trust it once you import it, which the last step offers.",
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
  "access.suffixes": "App domains",
  "access.suffixesLead": (example: string) =>
    `A new service's URL is suggested as ${example}. Each domain needs a wildcard DNS record pointing at this server.`,
  "access.suffixesNone":
    "New services get no suggested URL, so people type the whole domain. Add a domain you own to suggest one.",
  "access.privateAsk": (address: string) =>
    `This server only has the private address ${address}. Use sslip.io URLs anyway? Only computers on the same network reach them, and the CA made on this server signs their certificates.`,
  "access.privateUse": (suffix: string) => `Use ${suffix}`,
  "access.privateDomain": (address: string) =>
    `This server only has the private address ${address}, so Let's Encrypt can't check sslip.io URLs. Add a domain you own, or choose Test machine.`,
  "access.suffix": (n: number) => `App domain ${n}`,
  "access.suffixDefault": "Default",
  "access.suffixRemove": (name: string) => `Remove ${name}`,
  "access.suffixAdd": "Add a domain",
  "access.recheck": "Check again",
  "access.suffixSslip": "Points at this server with no DNS setup.",
  "access.suffixMissing": (wildcard: string) => `No ${wildcard} record yet.`,
  "access.suffixRecord": "Add this record at your DNS provider:",
  "access.recordType": "Type",
  "access.recordName": "Name",
  "access.recordValue": "Value",

  // Components
  "components.title": "Choose components",
  "components.description":
    "The components installed by default are already checked. Add or remove any; what a component needs is added with it.",
  "components.chosen": (n: number) => `${plural(n, "component", "components")} chosen`,
  "components.needed": "Needed by another component",
  "components.privileged": "Privileged",
  "components.privilegedHint":
    "Runs with host access, so its namespace is exempt from the restricted Pod Security level.",
  "components.personalDescription":
    "Ingress and certificates come by default. Tick Zeabur Builder if you want to build apps from Git or a folder. What a component needs is added with it.",
  "components.notPersonal": "Not in a personal install",
  "components.needsMemory": (mib: number) =>
    `Needs ${mib.toLocaleString("en")} MiB of memory to build apps, and this server has less`,
  "components.unsupported": (distribution: string) => `Not on ${distribution}`,
  "components.runsOn": (where: string) => `Runs on ${where} only`,
  "components.unsupportedOn": (profile: string) => `Not supported on ${profile}`,
  "components.details": (name: string) => `About ${name}`,
  "components.customise": (n: number) => `Customize (${n})`,
  "components.settings": "Settings",
  "components.required": "Required",
  "components.generated": "Generated when left empty.",
  "components.list": "Write a list: [a, b].",
  "components.secret": "Saved to a file only root can read, never in the configuration.",
  "components.incomplete": (names: string) => `Fill in the settings of ${names} to continue.`,
  "note.builds/zeabur-builder":
    "Builds run in a privileged buildkitd, so a build that escapes its sandbox reaches the node. Only admins should have access to the builder-system and zot namespaces.",
  "note.volume-backup/k8up": "Backs up volumes to S3. Database backups don't need it.",
  "note.volume-backup/velero": "Backs up the whole cluster to S3. Database backups don't need it.",
  "velero.endpoint": "Endpoint",
  "velero.endpointHint":
    "The S3 address of your storage. For AWS S3, https://s3.<region>.amazonaws.com.",
  "velero.bucket": "Bucket",
  "velero.region": "Region",
  "velero.regionHint": "us-east-1 when left empty. Cloudflare R2 uses auto.",
  "velero.accessKey": "Access key ID",
  "velero.secretKey": "Secret access key",
  "input.volume-backup/velero.s3-url": "Endpoint",
  "input.volume-backup/velero.bucket": "Bucket",
  "input.volume-backup/velero.credentials": "Credentials file",
  "inputHelp.volume-backup/velero.credentials":
    "An AWS credentials file: [default], then aws_access_key_id and aws_secret_access_key lines.",
  "input.volume-backup/velero.repository-password": "Volume copy password",
  "inputHelp.volume-backup/velero.repository-password":
    "Kopia encrypts the copies of volume files with it.",
  "input.volume-backup/velero.backup-key": "Backup encryption key",
  "inputHelp.volume-backup/velero.backup-key":
    "The S3 server encrypts every backup with this key (SSE-C). It must be 32 characters.",
  "input.volume-backup/velero.file-backup": "Copy volume files",
  "inputHelp.volume-backup/velero.file-backup":
    "Adds an agent on every node that copies the files in volumes. It runs as root with access to the host.",
  "input.remote-access/tailscale-operator.oauth-client-id": "OAuth client ID",
  "inputHelp.remote-access/tailscale-operator.oauth-client-id":
    "Make an OAuth client in the Tailscale admin console with write access to Devices Core, Auth Keys and Services, and the tag tag:k8s-operator. In the tailnet policy, tag:k8s-operator must own tag:k8s.",
  "input.remote-access/tailscale-operator.oauth-client-secret": "OAuth client secret",
  "inputHelp.remote-access/tailscale-operator.oauth-client-secret":
    "The secret of the same OAuth client.",
  "input.intrusion-prevention/crowdsec.bouncer-key": "Bouncer key",
  "inputHelp.intrusion-prevention/crowdsec.bouncer-key":
    "Traefik's bouncer calls CrowdSec's local API with it.",
  "input.intrusion-prevention/crowdsec.lapi-secret": "Local API secret",
  "inputHelp.intrusion-prevention/crowdsec.lapi-secret":
    "CrowdSec's local API signs agent sessions with it.",
  "input.intrusion-prevention/crowdsec.registration-token": "Registration token",
  "inputHelp.intrusion-prevention/crowdsec.registration-token":
    "CrowdSec's agents register with the local API with it.",
  "capability.certificates": "Certificates",
  "capability.builds": "Builds",
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
  "capability.databases": "Databases",
  "capability.external-secrets": "Secret sync",
  "capability.image-updates": "Image updates",
  "capability.object-storage": "Object storage",
  "capability.replicated-storage": "Replicated storage",

  // Review
  "review.title": "Review",
  "review.description": (path: string) =>
    `This is what will be installed. The configuration is saved to ${path}, so the same install can be repeated from the command line.`,
  "review.summary": "Summary",
  "review.kubernetes": "Kubernetes",
  "review.cluster": (distribution: string, version: string) => `${distribution} ${version}`,
  "review.clusterCustom": (distribution: string, version: string) =>
    `${distribution} ${version} (unsupported)`,
  "review.kubesolo": (version: string) => `KubeSolo ${version}`,
  "review.personalAccess": "zeaburctl ui on your own computer",
  "review.personalLimits":
    "Whoever has the kubeconfig has full control. Apps are not isolated from each other.",
  "review.access": "Access",
  "review.suffixes": "App domains",
  "review.suffixesNone": "None: people type each service's whole domain.",
  "review.components": "Components",
  "review.memory": "Memory at idle",
  "review.time": "Time",
  "review.timeValue": "10 to 20 minutes",
  "review.after": "After installing",
  "review.can": (items: string[]) => `You can ${list(items)}.`,
  "review.gitMissing":
    "Deploying from Git needs Zeabur Builder, which this catalog doesn't have yet.",
  "review.gitNeeds": (names: string[]) => `Deploying from Git needs ${list(names)}.`,
  "review.backupsLater": "Backups need an S3 bucket, which you set up in Zeabur after installing.",
  "review.dbBackupsLater":
    "Database backups need an S3 bucket, which you set up in Zeabur after installing.",
  "review.features": "What each component adds",
  "review.notChosen": "Not chosen",
  "review.notInCatalog": "Not in this catalog yet",
  "review.inZeabur": "In Zeabur, after installing",
  "can.image": "deploy apps from images",
  "can.git": "deploy from Git",
  "can.postgres": "create PostgreSQL databases",
  "feature.git": "Deploy from Git",
  "feature.updates": "Image updates",
  "feature.postgres": "PostgreSQL",
  "feature.usage": "Usage history",
  "feature.logs": "Log history",
  "feature.backups": "Database backups",
  "review.warnings": "Before you install",
  "review.plan": "Full plan",
  "review.config": "Configuration file",
  "review.details": "Details",
  "review.charts": "Charts and versions",
  "review.install": "Install",
  "review.blocked": "Fix the failed server checks first.",
  "review.blockedMemory": "These components need more memory than this server has.",
  "review.blockedTitle": (n: number) => `${plural(n, "server check", "server checks")} failed`,
  "review.blockedHint": "Fix these on the server, then check again.",
  "review.backToChecks": "Back to server checks",
  "review.leftOut": "Left out",
  "review.leftOutItem": (name: string, reason: string) => `${name} (${reason})`,
  "review.privileged": (n: number) => `Privileged: ${n}`,
  "review.byDefault": (n: number) => `${n} installed by default`,
  "review.added": (n: number) => `${n} added by you`,
  "review.needed": (n: number) => `${n} needed by other components`,
  "review.group.default": "Installed by default",
  "review.group.enabled": "Added by you",
  "review.group.dependency": "Needed by other components",
  "review.group.leftOut": "Left out",
  "review.col.component": "Component",
  "review.col.chart": "Chart",
  "review.col.version": "Version",
  "review.col.memory": "Idle memory",
  "review.manifests": "Manifests",
  "review.program": "Experience program",
  "review.program.label": "Join the experience program to help improve Zeabur",
  "review.program.description":
    "Sends the version and how many people use Zeabur each day, so we know how many installations are in use and what to fix first. No names, domains or emails, and IP addresses aren't kept. You can turn it off any time.",
  "review.program.link": "See what is sent",
  "review.removing": "Components to remove",
  "review.removingHint":
    "helm-controller uninstalls their charts. What stays can be deleted by hand later.",
  "review.stays": (items: string[]) => `Stays: ${list(items)}`,
  "review.stays.namespace": (namespace: string) => `namespace ${namespace}`,
  "review.stays.crds": "its CRDs and their objects",
  "review.stays.volumes": "its disks",
  "review.stays.manifests": (name: string) => `the objects of ${name}`,
  "review.deletesCrds": (crds: string[]) =>
    `Deletes these CRDs and all their objects: ${list(crds)}`,
  "review.inUse": (items: string[]) =>
    `Still in use by ${list(items)}. Remove those first, or keep the component.`,
  "review.losesData": (name: string) => `Remove ${name} and delete the data it keeps`,
  "review.blockedRemoval": "A component to remove is still in use or needs your confirmation.",

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
    `${plural(n, "component has", "components have")} no measurement yet, so actual use may be higher.`,

  "warning.kubernetes.untested": (version: string, tested: string) =>
    `The catalog was tested on ${tested}, not on Kubernetes ${version}.`,
  "warning.metrics.missing": () => "Without VictoriaMetrics, Zeabur shows no usage history.",
  "warning.domains.none":
    "This server has no public address, so new services get no suggested URL. Add an app domain you own, with a wildcard record pointing at the server.",
  "warning.domains.sslip": (suffix: string) =>
    `This server has no public address, so new services get no suggested URL. Add ${suffix} as an app domain for URLs only this network reaches.`,
  "warning.local.dns": (identity: string, zeabur: string) =>
    `${zeabur} and ${identity} must resolve to this server through DNS, for browsers and for the server itself. A hosts file is not enough: pods do not read it.`,
  "warning.personal.isolation": () =>
    "A personal install is for one person. Whoever has the kubeconfig has full control, and apps are not isolated from each other: KubeSolo ignores network policies. For a team, install with K3s or RKE2 on a server with 4 GiB of memory.",
  "warning.memory.personal.low": (host: string) =>
    `A personal install needs at least 1.5 GiB of memory; this machine has ${mib(host)}.`,
  "warning.memory.personal.tight": (idle: string, host: string) =>
    `At idle, KubeSolo and the components use about ${mib(idle)} of this machine's ${mib(host)}.`,
  "warning.memory.tight": (idle: string, host: string, distribution: string) =>
    `At idle, the system, ${distribution} and the components use about ${mib(idle)} of this server's ${mib(host)}.`,
  "warning.cluster.unsupported": (distribution: string, version: string) =>
    `${distribution} ${version} is not supported by this installer: its downloads are checked only against the release's own checksums.`,
  "warning.join.sameRegion": () =>
    "Put every node at the same hosting provider and in the same region. Different availability zones in one region are fine. Between providers or regions, latency slows etcd, the Kubernetes API and storage, and can make the cluster unstable.",
  "warning.join.rtt.high": (median: string, limit: string) =>
    `The server is ${median} ms away (median of 5 TCP connects to port 9345); within a region it is usually under ${limit} ms.`,
  "warning.join.rtt.critical": (median: string, limit: string) =>
    `The server is ${median} ms away (median of 5 TCP connects to port 9345). Above ${limit} ms the cluster will be slow and unstable.`,
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
  "phase.cluster": (distribution: string, version: string) =>
    `Install Kubernetes (${distribution} ${version})`,
  "phase.kubesolo": (version: string) => `Install Kubernetes (KubeSolo ${version})`,
  "phase.helmController": "Helm controller",
  "phase.namespacesOnly": "Namespaces",
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
  "done.personalTitle": "Your cluster is ready",
  "done.personalDescription":
    "Copy the kubeconfig to your own computer and open Zeabur there with zeaburctl ui. Stop the installer with Ctrl-C when you are done.",
  "done.personal": "On your computer",
  "done.personalHint": "Copy the kubeconfig from this server, then open Zeabur with it.",
  "done.personalKey":
    "The kubeconfig has full control of the cluster. Keep it to yourself, like an SSH key.",
  "done.setup": "First administrator",
  "done.setupHint": "This link works once, within 24 hours.",
  "done.register": "Register a passkey",
  "done.tipSaved": "The passkey is saved on this computer or on your phone.",
  "done.tipSecond": "Register a second passkey too, in case you lose the first.",
  "done.tipEveryday":
    "Open a separate everyday account from Members, and keep the admin account for setup.",
  "done.tipExpired": "Link expired? Run this on this server for a new one.",
  "done.open": "Open Zeabur",
  "done.zeabur": "Zeabur",
  "done.identity": "Sign-in",
  "done.descriptionLocal":
    "Trust this server's certificate authority first, then register the first administrator's passkey. Stop the installer with Ctrl-C when you are done.",
  "done.caStep": "1. Trust the certificate authority",
  "done.setupStep": "2. Register the first administrator",
  "done.caHint":
    "Zeabur, sign-in and your apps' domains all use certificates from this authority. Import it into your system or browser and trust it for websites. Until then, browsers warn about these sites and passkey registration fails.",
  "done.caDownload": "Download certificate",
  "done.noLink":
    "Zeabur was already set up, so no new link was made. Run sudo zeabur-installer setup-link for one.",
}

export type Messages = typeof en
