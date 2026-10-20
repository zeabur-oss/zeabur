# Zeabur interface design

Zeabur's own rules for the web interface. Components, tokens and fonts come from
[@zeabur/design-system](https://github.com/zeabur/design-system) (preview them in its
Storybook): shadcn/ui's components on Base UI with Zeabur's variants, shadcn/ui's theme (preset
brrM) and Zeabur's tokens. Build from those components (Badge, Card, Table, ToggleGroup,
Tooltip…) and compose Zeabur's own visuals on top rather than hand-rolling look-alikes.
`src/styles/app.css` imports the design system and adds only this interface's layout tokens. The
design system's tokens:

- **agent-playground's and lymeno's language** (Linear's, measured from its desktop app): two
  grounds, the window `--chrome` under the sidebar and one rounded panel (`--background`) on it,
  with white cards, fields and popups in the panel; near-hueless greys; 0.5px hairlines (`border`
  is 0.5px); faint layered shadows (`shadow-xs` on what sits on the panel, `shadow-md` on what
  floats, `shadow-xl` on dialogs). **Lime and ink** are Zeabur's accent: lime with ink text on solid
  buttons and switches, ink for the logo (lime on dark), lime under ink for the selection;
  `brand-strong` (lime deepened to 4.5:1) for a check, a link and a focused field's edge. Never a
  status.
- **Status**: `--ok` (green), `--progress` (blue), `--warn` (orange), `--fail` (red), `--idle`
  (grey), each with `-soft` (pill fill) and `-strong` (pill text). Status is a pill (Badge, dot
  and one word) where there is room and a bare round dot in dense places, with its word in a
  tooltip.
- **Surfaces and fills**: the window ground `--chrome` around the panel, `--plane` (a ground for
  things laid out in space, a segmented control's groove), `--rule` (hairlines between rows);
  hover, a selected row and a press are translucent fills over any ground (`bg-hover` 3.5%,
  `bg-selected` 6%, `bg-press` 8%) that show at once and fade when the pointer leaves.
- **Shape**: controls 28 / 32 / 40px tall and rounded 8 (`rounded-lg`); cards rounded 8 with a
  hairline and `shadow-xs`; small parts (chips, inline links, a count) rounded 6 (`rounded-md`);
  the panel, menus, popovers, dialogs and dashed state blocks rounded 12 (`rounded-xl`); icon
  buttons, avatars, status dots and counts round (`rounded-full`).
- **Type**: Inter, 13px at 450 for text: `type-title` 24/32, `type-heading` 15/22,
  `type-body` 13/20, `type-meta` 12/18, `type-micro` 11/16, `type-nano` 10/14 (counts, axes, key
  hints), `type-figure` 18/24, all in every language; `type-mono` for machine values only, in
  JetBrains Mono 12 (both self-hosted by the design system). Han text falls back to the system's
  CJK face for the locale; `text-autospace` spaces it from Latin and digits. 500 for emphasis
  (labels, buttons, sidebar rows), 600 for titles. Figures are tabular everywhere.
- **Motion**: the sidebar folds with Linear's easing; active markers slide (`slide`: segmented
  controls, the tabs' pill); popups fade and grow a little from their trigger; panels come in
  from their edge (the quick look `enter-from-right`, a `panel` or `sheet` dialog); toasts slide in;
  `prefers-reduced-motion` turns it off.
- **Buttons** are the design system's: 13px at 500, sentence case. Default is lime with ink text,
  outline is white with a hairline, ghost and muted are for toolbars and rows, icon buttons are
  round.
- **Replicas** are said in words first, "2/3" (ready out of desired), then one small square per
  replica in its state, at most 10 and the rest counted; only in tables, where each square opens
  that pod's logs. No field of dots without words.
- **Bars** are the design system's `Meter` (a value on a track, a request as a mark) or
  `MeterStack` (parts of a whole), so every bar is the same: a 6px round-ended track at 8% of the
  text colour, filled in blue at 70%, orange near a limit, grey for what is reserved. No bar of
  its own.
- **Segmented** switches show the choice on a white thumb in a grey groove (a lighter grey in
  dark mode). Tabs are the same: the dashboard's TabSwitcher (`Tabs`), the only tab style; a
  service's tabs fill its column, each with its icon, no counts, and scroll sideways when they do
  not fit. Vertical tabs are drawn as sidebar rows.
- **Columns**: a service's header, tabs and content sit in Zeabur's 960px column
  (`max-w-content`), centred; Home, Projects and the project's lists in a wider one
  (`max-w-6xl`); project settings in the dashboard's settings column (`max-w-4xl`); the
  Architecture canvas takes the whole pane.
- **Pages** follow cloud consoles (GCP, Volcengine): every page starts with the design system's
  `PageHeader` (one title size; a pill or version beside the title, actions or meta such as
  "Updated" at the end of its row). An overview puts its key figures right under it in a
  `StatGroup`, from the left, before anything drawn; lists are tables in a bordered card with
  Table `ruled` (the heading row on a light fill). A name, a namespace or an object never breaks
  onto a second line: it truncates, with the whole of it in `title`.
- **Illustrations** are the landing site's mosaic tiles (the design system's `Illustration`): a
  state that fills a page or a section (no projects yet, no releases, no objects, an empty or
  unmatched log, not found, refused, a route that failed) is an `Empty` with its illustration and
  one line. A "None." inside a card stays text, and "nothing needs attention" stays a dot and a
  line: a picture there would draw the eye to nothing. No other illustration style, no icons as
  pictures.
- **Choices** are the design system's `Select`, whose list is the menu; never a native `<select>`,
  whose list the browser draws.
- **Focus**: no focus rings, as in agent-playground and Linear (a product decision). A field
  shows where typing goes by its edge (`focus-within:border-ring` on a field-like wrapper); a list
  or menu by its highlighted row. Keyboard use itself stays: Tab order, Esc, arrow keys.
- **Not used** (`scripts/check-ui-styles.sh` fails on them): all caps and wide tracking,
  gradients, backdrop blur, Phosphor `duotone`, focus rings (`focus-visible:ring-*`,
  `focus-visible:outline-*`), a kind after a heading, middle-dot strings, bordered pills for facts;
  also no hover lifts, no icon tiles, no `→` after link text (see Copy).

Icons: Phosphor only (`import XIcon from "~icons/ph/x"`, `src/lib/icon.ts`), regular weight, bare,
`aria-hidden` when decorative, 14px in rows and buttons; the design system draws their lines at
about 1.2px, as agent-playground's.

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
- **Static classes** (`require-static-classes`): a component's `className` is literal strings or
  `cn` of literals; pick a variant rather than indexing a lookup table.

## Depth

Zeabur has depth. A person new to it does most of what they came for (is it running, read its
logs, change a variable, scale it, give it a domain) on a few plain pages; what takes knowing
Kubernetes is one deliberate step further. Decide a feature's layer before its place:

| Layer         | For                        | Global         | Project                      | Service                                                                    |
| ------------- | -------------------------- | -------------- | ---------------------------- | -------------------------------------------------------------------------- |
| **Everyday**  | everyone                   | Home, Projects | Overview, Services, Settings | Overview, Logs, Variables, Metrics, Networking, Storage, Settings' Compute |
| **Adjusting** | digging in, tuning         |                | Architecture                 | Events, Releases (and rollback), Settings' Health checks                   |
| **Expert**    | people who know Kubernetes | Cluster        | Objects                      | Terminal, YAML                                                             |

- **Everyday comes first and alone.** The sidebar leads with Home and Projects; Cluster and
  Settings are in a Manage group after them. A project's sidebar lists Overview, Architecture
  and Settings, then its services, with Objects under Advanced. A service shows seven tabs, then
  Events while it has any and Releases where they apply; Terminal and YAML open from the header's
  menu. There is no "More" menu (Zeabur's dashboard has none): what does not apply is left out.
  In a form, an adjusting part is a folding section (Collapsible `section`).
- **Fold, never hide.** Every deeper page stays one press away: a folded group remembers being
  opened (`remembered` in `lib/stores.ts`), a group that holds the page shown opens itself, and
  a link straight to a deeper page (`?tab=yaml`, a toast's View, the header menu's View YAML)
  shows it in place, as a tab of its own while it is open.
- **One current row.** The deepest place the page is has the selected fill; the rows it is under are
  ancestors (label in the text colour). Navigation links match their path exactly.

## Information architecture

Find the problem first, then let the person choose to dig into a service or to understand how the
project fits together. Workflow, comparison and relationships each have their own entry.

| URL                                 | Page                                                                                                                                                                                                                                                                                                                                                                                                         |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `/`                                 | **Home**: key figures (services healthy, failing, not ready, rolling out; the cluster's version, nodes ready and upgrade pill, linking to `/cluster`), needs attention (every unhealthy service across projects with its Logs and Events), projects failing first (services healthy out of all with what needs attention in a pill, warnings of the last hour, domain; 20 shown), recent warnings (5).       |
| `/cluster`                          | **Cluster**, tab Overview: version with an upgrade pill, needs attention, nodes with requests against allocatable (usage when metrics-server is there), the last hour's warnings about the cluster itself (no namespace, a Node, or a `kube-` namespace). Projects are Home's. Each section loads alone; a section the person may not read names the missing rule and copies a ClusterRole.                  |
| `/cluster/components`               | **Cluster**, tab Components: installed components with their versions against the tested ones, then the catalogue. Reading it takes an administrator's rights; anyone else sees the missing rules and a ClusterRole to copy.                                                                                                                                                                                 |
| `/projects`                         | Projects as Zeabur's project cards (namespace, services' health, domain; failing first), or as a list (remembered), then the other namespaces.                                                                                                                                                                                                                                                               |
| `/projects/:p`                      | **Overview** (default): name and health, key figures, Failing (failing pod → its Logs and Events), services table (failures first; live CPU and memory when metrics-server answers), events (`?events=all`), domains, storage.                                                                                                                                                                               |
| `/projects/:p/architecture`         | **Architecture**: layered canvas, declared (solid) and inferred (dashed) edges, PVCs under cards; a card opens a quick look (`?service=Kind/name`).                                                                                                                                                                                                                                                          |
| `/projects/:p/services`             | **Services**: full table; `?q`, `?status=attention\|healthy`, `?kind`, `?sort`, `?desc`. A name opens the service; a replica's square opens that pod's logs.                                                                                                                                                                                                                                                 |
| `/projects/:p/objects`              | **Objects**: kinds first. The usual kinds and pinned kinds are read on open; every other served kind (custom resources too) is a row read on click; Scan all kinds reads the rest; kinds without access are marked; answers kept 45 s, Refresh re-reads, never polled; generated (`?generated`) and empty (`?empty`) kinds folded; `?q`; a name opens its YAML (`?object=group/version/resource/name`).      |
| `/projects/:p/services/:kind/:name` | **Service**: header (icon, name, status, created, `Kind/name`, image, Restart and a menu with Open terminal, View YAML, Delete), then tabs Overview, Logs, Variables, Metrics, Networking, Storage, Settings, with Events while there are any and Releases for workloads that keep them; Terminal and YAML while open (`?tab`, `?pod`). Metrics: Usage (live, per replica) and History (`?range`, `?split`). |
| `/projects/:p/settings`             | **Project settings** (their own sidebar: Back, General): Basic information (icon, name, description), Kubernetes namespace (name, shown as a project or Adopt), Batch actions (pause or restart every service, one plan each), Danger zone (Remove from Zeabur).                                                                                                                                             |
| `/settings`                         | Preferences, the metrics store usage history is read from, the signed-in person and About. `/settings/components` redirects to `/cluster/components`.                                                                                                                                                                                                                                                        |

Everything a person picks (tab, filter, sort, selected service, pod) is in the URL: links are
shareable and Back works. The Architecture viewport is kept per project for the visit, so coming
back from a service page finds the canvas as it was.

## Shell

The frame is Zeabur's dashboard, from the design system (`AppFrame`, `FrameSidebar`, `Sidebar`):
the navigation drawer (224px) beside the page, which scrolls on its own. There is no top bar and no
breadcrumb: the sidebar says where the page is, and ⌘K jumps anywhere.

- **Sidebar**: the logo; where the dashboard has its workspace picker, the cluster (its name, a
  link to `/cluster`, `SidebarPicker`); the search row (`SidebarSearch`, ⌘K); Home and Projects,
  then Cluster and Settings under Manage; at the foot, "Admin until 14:05" while admin rights are
  in use, and the person's menu (`SidebarPicker` as a menu trigger): who is signed in, Use or Drop
  admin rights, the language, the theme, and Sign out where people sign in. Rows are 32px, a 14px
  icon and a 13px label at 500; the current one is raised on the white pill.
- **Project sidebar**, which replaces the global one on every project page: back to Projects, the
  project's name (with its icon and a switcher) and description, the search row, Overview,
  Architecture and Settings, then the services under a Services label (its action opens the full
  Services table), grouped by `part-of` or by role (Web, Internal, Workers, Jobs, Data) as folding
  rows (`SidebarMenu`) with each service on the tree rail and a status dot; one group is listed
  flat. Objects and the objects that belong to no service are under Advanced, folded. The
  namespace is in a pill to copy at the foot (`SidebarCopyPill`). On the settings pages the
  sidebar is their navigation instead: Back, the title, General.
- **Phones** (< 768 px): the dashboard's header (menu, logo, search) above the page; the menu opens
  the sidebar (global or the project's) in a sheet from the left.
- **⌘K palette**: projects, services, pods (their logs), domains, a few actions, recent items.
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
configured") and says no more. The Architecture lines come from Kubernetes objects and variable
values, not from traffic: no animation, a legend, and the reason for each edge on hover. Where the
project's NetworkPolicy objects block an edge it is drawn dotted in the failure colour (warning
colour when only partly allowed); Networking's Reachability section says the same per service and
always adds that enforcement depends on the cluster's network plugin. Backups (Storage tab,
Overview) show each claim's and database's schedule and latest run as the provider reports them,
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
changed and a View action to where it shows (the Releases tab after a restart or rollback, the
Variables tab, the object), or Undo where the change can be taken back as a change of its own
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

Transitions are short and decorative only; `prefers-reduced-motion` turns them off. No focus
rings (see the design system's tokens above); scrollable regions (the log) still take focus.

Movement is written so that it happens only without that preference: the `slide` and
`enter-from-right` utilities (the design system's) inside `prefers-reduced-motion: no-preference`, and
`motion-safe:` on the dialogs' slide. A marker that has a place to slide to is one element moved
by `translate`, never a highlight per item: Base UI's tab indicator (`--active-tab-*`) under Tabs,
and the thumb of `Segmented` (equal segments, so it moves by whole widths of itself and nothing
is measured). An entrance uses `@starting-style`, so no script waits for a frame. On top of that,
`app.css` shortens every transition and animation to nothing under the preference, for the parts
that come from libraries.
