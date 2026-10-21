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
- **Tabs** are the segmented `Tabs`, the only tab style; a service's six tabs fill its column,
  each with its icon (none on a phone), no counts. Vertical tabs are sidebar rows; a long page's parts are a
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

| Layer         | For                        | Global         | Project                         | Service                                                                                  |
| ------------- | -------------------------- | -------------- | ------------------------------- | ---------------------------------------------------------------------------------------- |
| **Everyday**  | everyone                   | Home, Projects | Overview (map or list)          | Overview, Logs, Deployments, Networking, Variables, Settings (Compute, Storage)          |
| **Adjusting** | digging in, tuning         |                | Suggestions, Activity, Settings | Settings' Health checks, Deployments' compare and roll back, the menu's Usage and Events |
| **Expert**    | people who know Kubernetes | Cluster        | Objects                         | The menu's Terminal and YAML                                                             |

- **Everyday comes first and alone.** The sidebar leads with Home and Projects; Cluster and
  Settings are in a Manage group after them. A project's sidebar lists Overview and Settings,
  then its services, with Objects under Advanced. A service has six tabs under the names hosted Zeabur uses:
  Overview (is it working, and if not why), Logs, Deployments (what changed), Networking,
  Variables and Settings. A tab is never also a header button. The header's one "More" menu
  (⋯) holds the views only people who know Kubernetes open (Usage, Events, Terminal, YAML) and
  Delete; a view shows in place of the tabs with Close. This menu replaced a fourth section,
  Inspect: people looked for logs and settings under the names they knew, and an expert view does
  not earn a tab. Nothing else goes in it; what does not apply is left out.
  In a form, an adjusting part is a folding section (Collapsible `section`). There is no
  beginner or expert switch: explanations attach to problems and first encounters, and nothing
  folds more than twice.
- **Fold, never hide.** Every deeper page stays one press away: a folded group remembers being
  opened (`remembered` in `lib/stores.ts`), a group that holds the page shown opens itself, and
  a link straight to a deeper page (`?tab=yaml`, a toast's View, the header menu's YAML) shows
  it in place.
- **One current row.** The deepest place the page is has the selected fill; the rows it is under are
  ancestors (label in the text colour). Navigation links match their path exactly.

## Information architecture

Find the problem first, then let the person choose to dig into a service or to understand how the
project fits together. Workflow, comparison and relationships each have their own entry.

| URL                                 | Page                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/`                                 | **Home**, "What needs you": one sentence of the overall state beside the cluster's version, nodes ready and upgrade pill (linking to `/cluster`); then the cluster's incidents, each once with the services and domains it takes down; then every other problem across projects grouped by where it comes from (the cluster, a cluster component, Zeabur, upstream, the settings, the app), each with its cause, who can fix it, and its fix, Logs and Events; cluster warnings that take nothing down in one line; what is rolling out. When nothing needs the person, one line. Then projects failing first (20 shown) and recent warnings (5).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `/cluster`                          | **Cluster**, tab Overview: version with an upgrade pill, needs attention, nodes with requests against allocatable (usage when metrics-server is there), the last hour's warnings about the cluster itself (no namespace, a Node, or a `kube-` namespace). Projects are Home's. Each section loads alone; a section the person may not read names the missing rule and copies a ClusterRole.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `/cluster/components`               | **Cluster**, tab Components: installed components with their versions against the tested ones, then the catalogue. Reading it takes an administrator's rights; anyone else sees the missing rules and a ClusterRole to copy.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `/projects`                         | Projects as cards (namespace, services' health, domain; failing first), or as a list (remembered), then the other namespaces.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `/projects/:p`                      | **Overview**: name and health, when it last heard from the cluster, "N suggestions" (missing setup, in a drawer, each with its fix), and Map or List (`?view`, remembered; a list from 13 services or on a phone). Needs attention: the project's incidents, then each failing service with its cause, fix and Logs. Map: layered canvas, declared (solid) and inferred (dashed) edges, PVCs under cards, a problem on its card; a card opens a quick look (`?service=Kind/name`); Activity beside it from the timeline (releases, writes by tool, warnings, terminations). List: the services table (`?q`, `?status=attention\|healthy`, `?kind`, `?sort`, `?desc`; live CPU and memory when metrics-server answers; a replica's square opens its logs), Activity, domains, storage and backups. An empty project leads with what to do next. `/projects/:p/architecture` and `/projects/:p/services` lead here.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| `/projects/:p/objects`              | **Objects**: kinds first. The usual kinds and pinned kinds are read on open; every other served kind (custom resources too) is a row read on click; Scan all kinds reads the rest; kinds without access are marked; answers kept 45 s, Refresh re-reads, never polled; generated (`?generated`) and empty (`?empty`) kinds folded; `?q`; a name opens its YAML (`?object=group/version/resource/name`).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| `/projects/:p/services/:kind/:name` | **Service**: header (icon, name, status with its reason, created, `Kind/name`, Restart, and the ⋯ menu with Usage, Events, Terminal, YAML and Delete), then six tabs (`?tab`). **Overview**, each card only when it has something: the health sentence first, with replicas and current usage; what is wrong (where it comes from, who can fix it, the fix, Logs, Terminal, Copy diagnosis); a rollout in progress or stuck; Networking, each domain and private address on one line: "Reachable", "Not reachable" with the failing hop in plain words and How to fix, or "Not verified from inside the cluster" when only hops Zeabur could not check remain; the hops wait behind "Show connection checks", where one not checked is neutral, never red; then warnings and connections. **Logs** (`?pod`): a replica that is gone gives way to the current one, which the page names. **Deployments**: the timeline (releases with their pods, compare and roll back; writes and the tool that made them; warnings; terminations). **Networking**, **Variables** and **Settings** (Compute, Health checks and Storage beside a section nav, `#part`): every edit is added to one pending plan ("2 changes pending · Review") whose dialog says what restarts, whether it fits and what is lost, and a tab holding one carries a dot. A menu view (`?tab=usage\|events\|terminal\|yaml`, `?range`, `?split`) shows in place of the tabs. An old `?tab=` (status, changes, configure, inspect with its `?view`, releases, metrics, a part of Settings) leads to its new place. |
| `/projects/:p/settings`             | **Project settings** (their own sidebar: Back, General): Basic information (icon, name, description), Kubernetes namespace (name, shown as a project or Adopt), Batch actions (pause or restart every service, one plan each), Danger zone (Remove from Zeabur).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `/settings`                         | Preferences, the metrics store usage history is read from, the signed-in person and About. `/settings/components` redirects to `/cluster/components`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |

Everything a person picks (tab, filter, sort, selected service, pod) is in the URL: links are
shareable and Back works. The map's viewport is kept per project for the visit, so coming back
from a service page finds the canvas as it was.

## Shell

The frame is the design system's (`AppFrame`, `FrameSidebar`, `Sidebar`): the navigation drawer
beside the page, which scrolls on its own. There is no top bar and no breadcrumb.

- **Sidebar**: the logo; the search row (`SidebarSearch`, ⌘K); Home and Projects, then Cluster
  and Settings under Manage; at the foot, the context chip (`ContextChip`): the cluster, who is
  signed in, and "Admin until 14:05" while admin rights are in use. Its menu links the cluster
  and holds Use or Drop admin rights, the language, the theme, and Sign out where people sign in.
- **Project sidebar**, which replaces the global one on every project page: back to Projects, the
  project's name (with its icon and a switcher) and description, the search row, Overview and
  Settings, then the services under a Services label (its action opens them as a list), grouped by `part-of` or by role (Web, Internal, Workers, Jobs, Data) as folding
  rows (`SidebarMenu`) with each service on the tree rail and a status dot; one group is listed
  flat. Objects and the objects that belong to no service are under Advanced, folded. The
  namespace is in a pill to copy at the foot (`SidebarCopyPill`), above the context chip. On the settings pages the
  sidebar is their navigation instead: Back, the title, General.
- **Phones** (below `md`): a header (menu, logo, search) above the page; the menu opens
  the sidebar (global or the project's) in a sheet from the left.
- **⌘K palette**: projects, services, pods (their logs), domains, verbs for each service (open
  its logs or a terminal, edit its variables, see its changes; matched in English too), a few
  actions, recent items.
  Enter while an IME is composing does nothing.

## Copy

Terse. Status is a dot plus one plain word ("● Ready 3/3", "● Keeps crashing"); the pod's own
reason (`CrashLoopBackOff`) and its restarts are in the pill's tooltip and in the details. Above
the fold a Pod is a replica, a probe a check (traffic, restart, startup); the Kubernetes words
stay in the kind chip, the YAML tab and "Details for Kubernetes users".
Counts are short ("8/9 ready", "14 restarts") and shown once: not in a heading, a pill and the
list below it all at the same time. No sentences explaining a chart or restating a title; an
explanation a first-time user needs goes in a tooltip or an (i). Empty states are one short line.
A search or filter box has no placeholder: its magnifying glass (or funnel) and its accessible name
say what it is. A placeholder stays only where it shows the format of a value (`250m`, `/healthz`)
or names one of two fields side by side (Name, Value).
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

Only real data. Where the API has nothing (usage without metrics-server, PVC usage, certificate
validity) the interface shows what it has (requests and limits, requested size, "HTTPS
configured") and says no more. The map's lines come from Kubernetes objects and variable
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
   "Scale to 4"), never a bare "Apply" unless the changes are mixed. The token's countdown shows
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
Networking. A workload or object managed by Helm, Argo CD or Flux keeps its edit controls
disabled with the reason in a tooltip; restart stays allowed.

Settings holds the forms (TanStack Form, loaded with the tab): compute (replicas; requests and
limits per container, with the largest node's allocatable beside them) and health checks (readiness,
liveness and startup, each with its effect in an (i) and a preset on the Service's port; they stay in
Settings because they also gate a rollout). Storage is edited where it is shown: each claim's card
on the Storage tab grows the claim and moves or protects its mount, and Add a volume sits below the
cards (the forms load after the cards). Modules add their own sections after Settings' forms
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
  rows that load or poll in, logs, charts, the map, tab panels and route changes do not move.
- Pages do not shift as they load: a route's loader reads what sits at the top of its page, a
  skeleton has the size of what replaces it, and a part that arrives later goes below the rest or
  holds its place.

No focus rings (see the design system's tokens above); scrollable regions (the log) still take
focus.
