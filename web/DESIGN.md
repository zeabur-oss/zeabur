# Zeabur interface design

Rules for the web interface. Components, tokens and fonts come from
[@zeabur/design-system](https://github.com/zeabur/design-system) (its README lists the tokens;
preview them in its Storybook). Build from its components and their variants rather than
look-alikes; `src/styles/app.css` imports it and adds only this interface's layout tokens.

- **Tokens**: zGray surfaces and text (`text-main`, `text-secondary`, `text-tertiary`,
  `bg-surface-page`, `border-main`, `*-v2-*`), Zeabur violet for actions and the current place,
  orange-red for danger, and shadcn/ui's theme variables mapped onto them, light and dark.
- **Status**: `ok`, `progress`, `warn`, `fail`, `idle`, each with `-soft` (pill fill) and
  `-strong` (pill text). A pill (Badge, dot and one word) where there is room, a bare dot in
  dense places with its word in a tooltip.
- **Type**: Geist and Geist Mono, self-hosted; the `type-*` roles (`type-title`, `type-heading`,
  `type-body`, `type-meta`, `type-label`, `type-micro`, `type-nano`, `type-figure`); `type-mono`
  for machine values only. Han text falls back to the system's CJK face; `text-autospace` spaces
  it from Latin and digits. Figures are tabular.
- **Motion**: the design system's motion utilities and components (its README, "Motion");
  nothing of this interface's own. See "Motion and focus".
- **Replicas** are said in words first, "2/3" (ready out of desired), then one small square per
  replica in its state, at most 10 and the rest counted; only in tables, where each square opens
  that pod's logs.
- **Bars** are `Meter` (a value on a track, a request as a mark) or `MeterStack` (parts of a
  whole); no bar of its own.
- **Tabs** are the segmented `Tabs`, the only tab style; a service's seven tabs fill its column,
  each with its icon (none on a phone, where the tabs also tighten so seven short labels fit; a row
  that still overflows scrolls with its far edge faded), no counts. Vertical tabs are sidebar rows; a long page's parts are a
  `SectionNav` beside it.
- **Columns**: a service's page in `max-w-content`; Home, Projects and the project's lists in
  `max-w-6xl`; project settings in `max-w-4xl`; the project's map takes the whole pane.
- **Pages** start with `PageHeader`. An overview puts its key figures under it in a `StatGroup`;
  lists are tables in a bordered card with Table `ruled`. A name, a namespace or an object
  truncates rather than wrapping, with the whole of it in `title`.
- **Empty states** that fill a page or a section are an `Empty` with its `Illustration` and one
  line; a "None." inside a card stays text.
- **Choices** are `Select`, never a native `<select>`.
- **Focus**: no focus rings. A field shows focus by its edge (`focus-within:border-ring`), a list
  or menu by its highlighted row; Tab order, Esc and arrow keys work.
- **Not used** (`scripts/check-ui-styles.sh` fails on them): `uppercase` and wide tracking,
  `bg-gradient-*`, backdrop blur, Phosphor `duotone`,
  focus rings (`focus-visible:ring-*`, `focus-visible:outline-*`), middle-dot strings, bordered
  pills for facts; also no hover lifts, no icon tiles, no `→` after link text (see Copy).

Icons: Phosphor only (`import XIcon from "~icons/ph/x"`, `src/lib/icon.ts`), regular weight,
`aria-hidden` when decorative, 14px in rows and buttons.

## Lint

`bun run lint` runs [shadcn/lint](https://github.com/shadcn-ui/lint) on app code; it recognises
the design system's components by their import (`componentImports` in `.oxlintrc.json`):

- **Variant over restyle** (`no-restyle`, layout allowed): a page may place and size a component
  (`flex`, `w-24`, `mt-2`, `relative`) but not change its colour, type, spacing or shape. A
  treatment used more than once becomes a variant or size in the design system in shadcn/ui's
  style (cva): Badge's status tones, Table `ruled`, Card `list`, Tabs `page`, ToggleGroup
  `segmented`, PopoverTrigger `ghost`/`plain`, Skeleton `card`. A one-off goes on a wrapper
  element.
- **Tokens, not arbitrary values** (`no-arbitrary-values`, `no-raw-colors`, `no-unknown-classes`):
  the `type-*` roles (plus `type-nano` 10/14 for counts, axes and key hints), the spacing scale
  (`gap-0.75`, not `gap-[3px]`), or a named token (`hairline-ring` from the design system; `grid-cols-settings`,
  `max-h-palette` in `app.css`). Add the token when the scale has nothing: to the design system if
  another app could use it, to `app.css` if it is a layout of this interface.
- **Dynamic values are custom properties** (`no-inline-styles`): `style` sets only `--*`
  properties, read by classes (`left-(--x)`, `translate-y-(--slide-y)`, `h-(--bar)`).
- **Design system components only** (`no-restricted-imports`): no `@base-ui/react`, Radix,
  Headless UI or Motion imports in app code; a missing component is added to the design system.
- **Guarded motion** (`zeabur/no-unguarded-motion`, from `@zeabur/design-system/lint`):
  `transition-transform`, `transition-all`, `animate-spin` and the like are written `motion-safe:`,
  or the motion is a design-system utility.
- **Static classes** (`require-static-classes`): a component's `className` is literal strings or
  `cn` of literals; pick a variant rather than indexing a lookup table.

## Depth

Zeabur has depth. A person new to it does most of what they came for (is it running, read its
logs, change a variable, scale it, give it a domain) on a few plain pages; what takes knowing
Kubernetes is one deliberate step further. Decide a feature's layer before its place:

| Layer         | For                        | Global         | Project                         | Service                                                                                |
| ------------- | -------------------------- | -------------- | ------------------------------- | -------------------------------------------------------------------------------------- |
| **Everyday**  | everyone                   | Home, Projects | Overview (map or list)          | Overview, Logs, Deployments, Usage, Networking, Variables, Settings (Compute, Storage) |
| **Adjusting** | digging in, tuning         |                | Suggestions, Activity, Settings | Settings' Health checks, Deployments' compare and roll back and its Events             |
| **Expert**    | people who know Kubernetes | Cluster        | Objects, the kubectl terminal   | the terminal, Deployments' Events and Settings' YAML                                   |

- **Everyday comes first and alone.** The sidebar leads with Home and Projects; Cluster,
  Members and Settings are in a Manage group after them. A project's sidebar lists Overview and Settings,
  then its services, with Objects under Advanced. A service has seven tabs under the names hosted Zeabur uses:
  Overview (is it working, and if not why), Logs, Deployments (what changed), Usage, Networking,
  Variables and Settings. A tab is never also a header button. A view only people who know
  Kubernetes open is a second segment in the tab it belongs to (`?mode=`): Deployments' Events,
  Settings' YAML; people look for them under the names they know, and an expert view does not
  earn a tab. A terminal is not a view of a page: it opens in the dock (see Shell). The
  header's one "More" menu (⋯) holds actions only (Copy diagnosis, Delete), never a view and
  never an action already a header button (Restart, Terminal); what does not apply is left out.
  In a form, an adjusting part is a folding section (Collapsible `section`). There is no
  beginner or expert switch: explanations attach to problems and first encounters, and nothing
  folds more than twice.
- **Fold, never hide.** Every deeper page stays one press away: a folded group remembers being
  opened (`remembered` in `lib/stores.ts`), a group that holds the page shown opens itself, and
  a link straight to a deeper page (`?tab=settings&mode=yaml`, a toast's View, an old `?tab=yaml`)
  shows it in place.
- **One current row.** The deepest place the page is has the selected fill; the rows it is under are
  ancestors (label in the text colour). Navigation links match their path exactly.

## Information architecture

Find the problem first, then let the person choose to dig into a service or to understand how the
project fits together. Workflow, comparison and relationships each have their own entry.

| URL                                 | Page                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ----------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `/`                                 | **Home**, "What needs you": one sentence of the overall state beside the cluster's version, nodes ready and upgrade pill (linking to `/cluster`); then the cluster's incidents, each once with the services and domains it takes down; then every other problem across projects grouped by where it comes from (the cluster, a cluster component, Zeabur, upstream, the settings, the app), each with its cause, who can fix it, and its fix, Logs and Events; cluster warnings that take nothing down in one line; what is rolling out. When nothing needs the person, one line. Then projects failing first (20 shown) and recent warnings (5).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `/cluster`                          | **Cluster**, tab Overview: version with an upgrade pill, needs attention, nodes worst first with use against allocatable when metrics-server is there (else requests), disk from the kubelets, the worst node named under the totals, and above 7 nodes a sort, a filter and a bound, the last hour's warnings about the cluster itself (no namespace, a Node, or a `kube-` namespace). Projects are Home's. Each section loads alone; a section the person may not read names the missing rule and copies a ClusterRole.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `/cluster/components`               | **Cluster**, tab Components: installed components with their versions against the tested ones, then the catalogue. Reading it takes an administrator's rights; anyone else sees the missing rules and a ClusterRole to copy.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `/projects`                         | Projects as a list by default (name and description, namespace, services' health, restarts, domain; failing first, `?sort=name`, `?desc`), or as cards (remembered); a filter above them (`?q`, `/` focuses it) by name, namespace or domain; then the other namespaces.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `/projects/:p`                      | **Overview**: name and health, when it last heard from the cluster, "N suggestions" (missing setup, in a drawer, each with its fix), and Map or List (`?view`, remembered; a list from 13 services or on a phone). Needs attention: the project's incidents, then each failing service with its cause, fix and Logs. Map: layered canvas, declared (solid) and inferred (dashed) edges, PVCs under cards, a problem on its card; a card opens a quick look (`?service=Kind/name`); Activity beside it from the timeline (releases, writes by tool, warnings, terminations). List: the services table (`?q`, `?status=attention\|healthy`, `?kind`, `?sort`, `?desc`; live CPU and memory when metrics-server answers; a replica's square opens its logs), Activity, domains, storage and backups. An empty project leads with what to do next. `/projects/:p/architecture` and `/projects/:p/services` lead here.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `/projects/:p/objects`              | **Objects**: kinds first. The usual kinds and pinned kinds are read on open; every other served kind (custom resources too) is a row read on click; Scan all kinds reads the rest; kinds without access are marked; answers kept 45 s, Refresh re-reads, never polled; generated (`?generated`) and empty (`?empty`) kinds folded; `?q`; a name opens its YAML (`?object=group/version/resource/name`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `/projects/:p/services/:kind/:name` | **Service**: header (icon, name, status with its reason, created, Restart, Terminal, and the ⋯ menu with Copy diagnosis, Copy name for kubectl and Delete; no `Kind/name` line), then six tabs (`?tab`). **Overview**, each card only when it has something: the health sentence first, with replicas and current usage; what is wrong, each cause once (the cause, a sentence where it lies when that is outside the service, the fix, Logs, Terminal, Copy diagnosis; when a cluster administrator or a component's owner must act, the fix opens with "This needs a cluster administrator:" and Copy diagnosis is the primary button; nothing is said when the reader can fix it), with what it also takes down in one "This also means" line (no ready replica is a consequence, not a second card) and a release stuck on it as "Deployment stuck" with Roll back; a rollout card only while it moves by itself; Networking, each domain and private address on one line: "Reachable", "Not reachable" with the failing hop in plain words and How to fix, "Waiting for web to be ready" (neutral) when the only failing hop is the replicas' readiness and the cause is above, or "Not verified from inside the cluster" when only hops Zeabur could not check remain; the hops wait behind "Show connection checks", where one not checked is neutral, never red; then the warnings no diagnosis explains, each in plain words with its reason beside it, and connections. **Logs** (`?pod`): a replica that is gone gives way to the current one, which the page names; its Terminal button opens a terminal in the replica shown, in the dock. **Deployments**: the timeline (releases with their pods, compare and roll back; writes and the tool that made them; warnings; terminations). **Networking**, **Variables** and **Settings** (Compute, Health checks and Storage beside a section nav, `#part`): every edit is added to one pending plan ("2 changes pending · Review") whose dialog says what restarts, whether it fits and what is lost, and a tab holding one carries a dot. A menu view (`?tab=usage\|events\|yaml`, `?range`, `?split`) shows in place of the tabs. An old `?tab=` (status, changes, configure, inspect with its `?view`, releases, metrics, a part of Settings) leads to its new place; an old `?tab=terminal` or `?mode=terminal` opens Logs with the service's terminal in the dock. |
| `/projects/:p/settings`             | **Project settings** (their own sidebar: Back, General): Basic information (icon, name, description), Kubernetes namespace (name, shown as a project or Adopt), Batch actions (pause or restart every service, one plan each), Danger zone (Remove from Zeabur).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `/members`                          | **Members**, people Zeabur Identity signs in: one line each (name, role, passkeys, last sign-in), a filter from 11; Invite shows a one-time link once (copy, expiry); a member opens in a panel with their passkeys and linked accounts; a new setup link, a reset after a lost device (removes passkeys, links and sessions first) and Remove (they lose access; projects and role bindings stay) each confirm with the cost first. Pending setup links, with Revoke. Every write is the person's own Kubernetes request; without Zeabur Identity, one line says so.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `/account`                          | **Account**, from the context chip's menu: who you are (name, user name, role, Kubernetes user), your passkeys (rename; remove, never the last; Add a passkey opens your own one-time setup link in a new tab), a nudge for a backup when there is one passkey, and linked accounts. Signed in from a kubeconfig, one line says there is nothing to manage.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `/settings`                         | Preferences, the metrics store usage history is read from, the signed-in person and About. `/settings/components` redirects to `/cluster/components`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |

Everything a person picks (tab, filter, sort, selected service, pod) is in the URL: links are
shareable and Back works. The map's viewport is kept per project for the visit, so coming back
from a service page finds the canvas as it was.

## Shell

The frame is the design system's (`AppFrame`, `FrameSidebar`, `Sidebar`): the navigation drawer
beside the page, which scrolls on its own. There is no top bar and no breadcrumb.

- **Sidebar**: the logo; the search row (`SidebarSearch`, ⌘K); Home and Projects, then Cluster,
  Members and Settings under Manage; at the foot, the context chip (`ContextChip`): the cluster, who is
  signed in, and "Admin until 14:05" while admin rights are in use. Its menu links your Account and the cluster
  and holds Use or Drop admin rights, the language, the theme, and Sign out where people sign in.
- **Project sidebar**, which replaces the global one on every project page: back to Projects, the
  project's name (with its icon and a switcher) and description, the search row, Overview and
  Settings, then the services under a Services label (its action opens them as a list), grouped by `part-of` or by role (Web, Internal, Workers, Jobs, Data) as folding
  rows (`SidebarMenu`) with each service on the tree rail and a status dot; one group is listed
  flat. Objects and the objects that belong to no service are under Advanced, folded. The
  namespace is not repeated there: it is in project settings (Kubernetes namespace, copyable) and
  ⋯ Copy name for kubectl. On the settings pages the
  sidebar is their navigation instead: Back, the title, General.
- **Terminal dock** (`Dock`): the terminals live under the page, beside the sidebar, and outlive
  every page and service change: a terminal keeps its connection and what it printed until its
  tab closes. A Terminal row at the sidebar's foot (Ctrl+\` anywhere, also from inside a
  terminal; a backtick alone still types) shows or hides it; ⌘K has Show terminal too. Its tabs
  are a tab list: a service's shell (named by the service, and the container when the replica has
  several), its debug container when the image has no shell, and the project's kubectl terminal
  ("kubectl shop"); Delete or × closes one, and closing the last kubectl terminal leaves its pod
  to the server's grace period. A service's terminal opens from its header's Terminal button, its
  Logs tab, a diagnosis or ⌘K; the kubectl one from ⌘K or the empty dock, through its plan the
  first time and on the running pod after. The empty dock lists the person's running kubectl
  terminals in the project, to open again after a reload or stop. A terminal closed by its
  credential's expiry (4000) or idleness (4001) keeps its tab, says why and offers Reopen on the
  same screen; nothing folds the dock but the person. Its height is dragged at its top edge or
  moved with the arrow keys there, and kept per viewer. Folded, with terminals open, it is a bar:
  how many, and when the first credential among them expires. Escape inside a terminal is the
  program's; elsewhere in the dock it folds it. On a phone the dock is a sheet over the whole
  screen. Its code (xterm.js) loads the first time it opens.
- **Phones** (below `md`): a header (menu, logo, search) above the page; the menu opens
  the sidebar (global or the project's) in a sheet from the left.
- **⌘K palette**: projects, services, pods (their logs), domains, verbs for each service (open
  its logs or its terminal in the dock, edit its variables, see its changes; matched in English
  too), a few actions (the kubectl terminal, show or hide the dock), recent items.
  Enter while an IME is composing does nothing.

## Copy

Terse. Status is a dot plus one plain word ("● Ready 3/3", "● Keeps crashing"); the pod's own
reason (`CrashLoopBackOff`) and its restarts are in the pill's tooltip and in the details. Above
the fold a Pod is a replica, a probe a check (traffic, restart, startup); the Kubernetes words
stay in the YAML tab and in the one "Details for Kubernetes users" disclosure at the end of a
section (`components/kube-details.tsx`), never as kind tags beside titles, rows or values
(docs/wording.md, "Section titles and terms").
Counts are short ("8/9 ready", "14 restarts") and shown once: not in a heading, a pill and the
list below it all at the same time. No sentences explaining a chart or restating a title; an
explanation a first-time user needs goes in a tooltip or an (i). Empty states are one short line.
A search or filter box has no placeholder: its magnifying glass (or funnel) and its accessible name
say what it is. A placeholder stays only where it shows the format of a value (`250m`, `/healthz`)
or names one of two fields side by side (Name, Value).
A list that can grow long (variables past 15, domains past 10, a group of services, older
releases) gets a filter (`components/filter-field.tsx`) once it is long, a bounded frame
(`max-h-90`, scrolling inside, its header sticky) where it would push the rest of the tab far
down, and a sentence above it that sums it up; a short list stays as it is.
A write's confirmation is the one place for whole sentences: what will happen, in words a
first-time user follows (see Operations); its toast is a word or two and the thing it names.
Kubernetes terms stay as Kubernetes spells them in every language.

Zeabur is for people who know Kubernetes, so the technical detail stays one step away, never in
the way ([progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/)):

- **Headings are plain words**: "Public", "Private", "Storage", not "Public · Ingress". The
  Kubernetes kind shows where an operator reads the object: a column (Networking's Via:
  `Ingress/shop`), a tooltip on the name (a claim is a PersistentVolumeClaim, a DNS name a
  Service), the details row under a page title (`Deployment  busybox:1.37`), the YAML tab.
  `Panel` and `Section` take no kind.
- **No middle-dot strings** ("A · B · C"). Values that need a label get one ("Cluster k3s
  Namespace shop"); facts in a sentence-like line are joined with the locale's separator
  (`t("common.sep")`: ", " or "，"); otherwise separate elements set apart by space.
- **Pills are for status** (`StatusPill`, Badge's status tones): a state that can change. A kind,
  a licence, a fact ("HTTPS configured" is the lock's tooltip), a category (Recommended) or
  "Declared" is plain text, or nothing when it is the default; "Inferred" is said, "Declared" is
  not.
- **Monospace for what you could paste**: names, images, hosts, keys and values, commands. A
  kind, a label or a column of kinds is in the text face.
- **No subtitle that repeats or explains the heading**, no eyebrow above it, no `→` after link
  text (arrows only for direction in data: domain → service, port → target).

`scripts/check-ui-styles.sh` fails on a `kube=` prop, a middle dot outside comments and a bordered
pill in small type. Translations: English; Traditional
Chinese with Taiwan usage (專案, 服務, 設定, 日誌, 變數, 網域, 元件); Simplified Chinese (项目,
服务, 设置, 日志, 变量, 域名, 组件).

## Data honesty

Only real data. Where the API has nothing (usage without metrics-server, volume and node disk use
when the kubelets' stats may not be read, certificate validity) the interface shows what it has
(requests and limits, requested size, "HTTPS configured") and says no more.

Fill leads with what is used, not what is reserved: a node's memory and disk, and a volume, show
used against allocatable or capacity, amber from 85% and red from 95% (a node's disk from 90%, where
the kubelet starts evicting); what pods reserve is the secondary figure and the bar's tick. Green
means checked: "nothing needs attention" only when every read behind it answered; otherwise the
line says what could not be checked. When the cluster does not answer, one banner on top names it,
when it last answered and the next try, the sections under it stay quiet, and Home shows the
projects as this browser last saw them, greyed. The same outcome on many nodes or incidents is one
line with one fix, the names behind a disclosure. The map's lines come from Kubernetes objects and variable
values, not from traffic: no animation, a legend, and the reason for each edge on hover. Where the
project's NetworkPolicy objects block an edge it is drawn dotted in the failure colour (warning
colour when only partly allowed); the Networking tab says the same per service and
always adds that enforcement depends on the cluster's network plugin. Backups (Settings'
Storage, the project's list) show each claim's and database's schedule and latest run as the provider reports them,
and say plainly when no backup component is installed or its objects may not be read.

## Operations

Next to an operation (Restart), the target and the environment are spelled out:
`Cluster k3s  Namespace shop  Deployment/worker`, each value after its label (the version is the
cluster page's, not repeated here).

Every write is planned first: a page describes the change (`usePlanner`), Zeabur answers with what
it will write, and a confirmation shows it in three layers
([progressive disclosure](https://www.nngroup.com/articles/progressive-disclosure/)):

1. **What will happen**, in plain sentences built from the change, the plan and the project
   (`components/plan/describe.ts`). The title already names the action, so the sentences say its
   effect, with the values that decide it, and never repeat the title: "A pod gets no traffic
   until connecting to port 8080 succeeds (checked every 10 s).", "api's 3 pods are replaced one
   at a time, so the service stays up.", what a variable or value change needs to take effect,
   and who else it touches ("Also affects web and worker (they read shop-config).").
2. **A button that names the action** ("Restart", "Save variables", "Roll back to revision 5",
   "Scale to 4"), never a bare "Apply" unless the changes are mixed beyond one draft ("Save"). The token's countdown shows
   only in its last two minutes; once it runs out, the button reviews the change again.
3. **Technical details**, folded: the YAML diff, the one place for the fields that change, and
   the fields other tools own. An object is named above its diff only when the plan changes
   more than one.

The weight follows the risk (`model/risk.ts`, one function per rule, unit-tested): a low-risk
change (a restart with at least two ready pods, scaling) gets the smaller dialog; a high-risk one
(a restart with fewer, scaling to zero, deleting a ConfigMap or Secret key, a rollback, any storage
change, a YAML edit, removing a domain, a policy that isolates) the wide one with the consequence
first, in a warning callout, and a destructive button. Everything else is the wide dialog without
the callout.

Sentences hold up to five changes (`components/plan/groups.ts`, `FEW`). Past that the dialog
leads with a fixed summary, conclusion first: what blocks it (in a failure callout, the button
disabled with the reason beside it), one aggregated risk callout, one line of counts ("1 removed,
6 changed, 32 added, stays up, fits") and one line of cost. Below, folding groups with their
counts: a draft by object, then part, each a Field, Before, After table (removed first, "—" for
none, a Secret's value hidden); a bulk action by outcome, each a Service, Replicas, Effect table,
with the exceptions open. A table scrolls inside a bounded height under a sticky header and turns
two-line on a phone; from twelve rows a filter box sits above. A sentence never lists more than
three names: the rest are "and N more" with Show all. Undo's toast is titled by its count ("Scale
45 services back up").

Confirming closes the dialog at once; the outcome is a toast (Sonner, one toaster in the shell,
loaded after the first page; `components/plan/outcome.ts`). Success says "Applied" with what
changed and a View action to where it shows (Deployments after a restart or rollback,
Variables, the object), or Undo where the change can be taken back as a change of its own
(scaling: Undo plans and applies the scale back). Failure says why in plain words: `409` "Changed
in the meantime" with Review again (the same change planned afresh, the dialog reopened), `403`
the permission the person's role lacks, `422` for an object Helm, Argo CD or Flux manages where to
change it, `429` to try again shortly; anything else passes the server's detail on.

Edits are made where the value lives: a literal variable in the Variables tab, a ConfigMap or Secret value
in that object's editor (a panel on the right, the key a variable reads highlighted), a domain in
Networking. Variables are edited together (`model/env-edit.ts`): Edit turns the list into a table
of names and values; rows are added at the bottom, removed with a strike-through and Undo, and
marked new, changed or to be removed, with a count. Pasting `NAME=value` lines into a name splits
them into rows, and the .env view becomes a text field (one container only) that reads quotes,
`export`, comments and blank lines. Repeated, invalid or already-set names are flagged where they
are typed and hold Save. Save stages one env change per container in the service's pending plan,
so its one review shows them all; Discard reverts; leaving the page with edits asks first. A
value from a Secret, a config file or Kubernetes is a read-only row naming its source in plain
words, with a link to where it changes; a Secret's value is masked until revealed, and the
review never shows it. A workload or object managed by Helm, Argo CD or Flux keeps its edit
controls disabled with the reason in a tooltip; restart stays allowed.

A container with no shell offers a debug terminal: Open debug terminal plans an ephemeral
container with a shell beside the app (the review says it stays until the replica is replaced),
waits for it, and attaches, in the same dock tab; a running one is reopened without a review. A
project whose cluster lets Zeabur hand over the person's credentials has a kubectl terminal in
the dock (an old `/projects/:p/terminal` link opens the project with it): a pod with kubectl and
helm acting as the signed-in person, planned first (the review says whose rights it has, how
long the credential lasts and that others with exec rights in the namespace can use it), listed
with Open and Stop, and ended when the credential expires. Running locally, none is offered.

Settings holds the forms (TanStack Form, loaded with the tab): compute (replicas; requests and
limits per container, with the largest node's allocatable beside them) and health checks (readiness,
liveness and startup, each with its effect in an (i) and a preset on the Service's port; they stay in
Settings because they also gate a rollout; the three share one card, a row each, with their fields
in fixed columns). These forms have no buttons of their own: what is typed waits in the page's draft
and is saved once from its bar. Storage is one list in the column's width (mount path, size, read-only,
status); Edit turns the rows into fields in place: a size only grows, the mount path moves, read-only
switches, a volume is removed with Undo (its claim and data stay), and Add a volume adds a row to the
same list, its name, claim name and access mode behind Advanced. The list's edits go through the
same draft and plan. Modules add their own sections after Settings' forms
through `modules/service-settings.ts`, an explicit list; their changes are their provider's change
types and still go through the confirmation. Releases lists the workload's templates newest first;
any one compares with the running one as a YAML diff, and Roll back plans a return to it (the
confirmation says ConfigMap and Secret contents are not rolled back). YAML is editable where it is shown
(a service's YAML tab, the Objects panel): Edit opens CodeMirror (its own chunk, loaded on first
use), and Review plans the edited object. Kinds Kubernetes makes itself (pods, events, ReplicaSets,
ControllerRevisions) stay read-only.

## Content Security Policy

The server sends `script-src 'self'` with Trusted Types and no inline script
(`crates/zeabur-ui`, `CSP`). Styles are `style-src 'self' 'unsafe-inline'`: Sonner inserts its
style sheet in a `<style>` element when the toaster loads and offers no nonce
([sonner#449](https://github.com/emilkowalski/sonner/issues/449)), and CodeMirror inserts its
theme the same way. `style-src` decides whether such elements apply
([MDN, style-src](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/style-src)),
so both load as their authors ship them, without build-time patches. shadcn/ui's Chart writes a
`<style>` through `dangerouslySetInnerHTML` only for colours given in its config; charts take
the theme's `--chart-*` in their marks instead, so it writes none (Trusted Types would refuse it). Inline styles can change how
the page looks, not run code. Our own markup still keeps styles in classes: `index.html` has no
`<style>` or `style=""` (`scripts/postbuild.ts`), and `style` props carry only custom properties.
Sonner inserts its sheet after `app.css`, so Zeabur's overrides of it (`app.css`, end) carry one
more selector.

## States

- **Loading**: skeletons shaped like the content (`LoadingRows`), announced politely.
- **Error**: `Problem` for every API error (RFC 9457), and a service's diagnoses, in five
  layers: what happened (naming the object), why (the rule, with this occurrence's numbers), "What
  to do" with one action, then the evidence (values and where they were read) and the raw answer
  ("Details for Kubernetes users", with a link to the problem type's page), both folded. Every
  layer is a keyed sentence in the reader's language. A toast says the first three only.
- **Missing permission** is a normal state, not an error (`components/refused.tsx`): a muted
  line where the data would be ("Seeing Secrets needs admin rights."), one action ("View as
  admin", "Retry as admin" where it re-runs a request, or "Use admin rights" where a permission
  hint said no before anything was asked) when admin rights are offered, else
  "Ask a cluster administrator.", and everything else (the refused request, kube-apiserver's
  answer, a role to hand over) behind one closed "Details". A refused kind shows in the section
  it feeds, never as a banner on top; a page has at most one such line for kinds with no section
  of their own. A whole area the person may not see is left out.

## Secrets

Values are never fetched with the page. "Reveal db-credentials" fetches one Secret on request
(`POST …/reveal`, `no-store`); values live in the tab's component state only and disappear on
leaving the tab or the service (the service route remounts its page for each service). YAML
(the Objects panel, a service's YAML tab, a plan's diff) never holds a value: the server writes
`(hidden)`. The Secret editor reveals on request the same way; a new value may be typed without
revealing the old one. In an edited YAML copy, a value left as `(hidden)` is kept as it is; the
server never writes the mask.

## Navigation and state

Each route's loader waits for the queries its page reads, so a page opens with its data; TanStack
Query keeps the cache and a loader never fails on the cluster's answer (the page shows a forbidden
or missing object in place). State that belongs to one project or service (a revealed Secret, the
log toolbar, the canvas viewport) resets when the project or service changes (`remountDeps`).
What a person picks to share or come back to (tab, pod, filters, sort, the quick look) is in the
URL; a search box writes what is typed with `replace` and starts again from the URL after Back,
Forward or a link.

## Motion and focus

Motion follows the design system's README, "Motion": which pattern moves how, its timings, what
never animates, and what is left under `prefers-reduced-motion` (opacity and colour only). Use
its utilities and components, not `transition-*` classes of your own. Here that means:

- Popups, dialogs, sheets, tooltips, toasts and collapsibles move as the design system's
  components do. A disclosure is `CollapsibleTrigger variant="disclosure"`, not a chevron of its
  own.
- An element Base UI does not mount takes `usePresence`, so its exit plays: the quick look
  (`sheet-motion-end`) and the draft bar (`overlay-motion`). The palette keeps its body mounted
  through the close, is anchored to the top so typing never moves it, and its fallback has its
  height.
- `SwapText` for one service's own status word and counts (the service header, the quick look,
  the status card, a rollout's progress); never for rows in a list or table.
- A row this tab just created (a variable, a domain, a volume) enters once (`lib/created.ts`);
  rows that load or poll in, logs, charts and the map do not move.
- A move to another page (another service, opening a project) fades the main panel in with the
  router's `page` view transition (`pageViewTransition`, or `usePageTransition` where the browser
  has no typed view transitions); the sidebar stays. A change of search only (a tab, a filter)
  is no page move: a tab panel fades in (`TabsContent`), nothing slides.
- Hovering a link to a service or project preloads it, so the transition shows a loaded page.
- Pages do not shift as they load: a route's loader reads what sits at the top of its page, a
  skeleton has the size of what replaces it, and a part that arrives later goes below the rest or
  holds its place.

No focus rings (see the design system's tokens above); scrollable regions (the log) still take
focus.
