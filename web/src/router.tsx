import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from "@tanstack/react-router"
import {
  pageViewTransition,
  viewTransitionTypes,
} from "@zeabur/design-system/components/page-transition"

import {
  capabilitiesQuery,
  clusterBackupsQuery,
  notificationsQuery,
  clusterQuery,
  clusterWarningsQuery,
  databasesQuery,
  eventsQuery,
  namespaceQuery,
  nodesQuery,
  projectGapsQuery,
  projectIncidentsQuery,
  projectQuery,
  projectsQuery,
  queryClient,
  recentWarningsQuery,
  rolloutQuery,
  toolsQuery,
  upgradesQuery,
} from "@/api/queries"
import { isUnreachable } from "@/api/client"
import { accountQuery, membersQuery } from "@/api/members"
import { AppShell } from "@/components/app-shell"
import { NotFound } from "@/components/not-found"
import { openKubectl, openShell, showDock } from "@/lib/dock"
import { RouteError, RoutePending } from "@/components/route-states"
import { canonicalKind, isTask } from "@/model/service"
import {
  formerPlace,
  validateComponentsSearch,
  validateObjectsSearch,
  validateProjectSearch,
  validateProjectsSearch,
  validateDatabaseSearch,
  validateServiceSearch,
} from "@/routes/search"

// Routes are defined in code; each page's component loads on first visit, so the first page load
// carries only the shell (scripts/ui-budget.sh).
//
//   /                                          Home: what needs you, grouped by where it comes
//                                              from, then the projects
//   /cluster                                   the cluster: version, attention, nodes, warnings
//   /cluster/backups                           Velero's backups: where they go, the schedule, each
//                                              run
//   /cluster/notifications                     where node and runtime-security alerts go
//   /cluster/components                        installed components and the catalogue
//   /projects                                  every project and namespace
//   /projects/$project                         the project: its problems, then its services as a
//                                              map or a list (?view=map|list, ?service=… quick look)
//   /projects/$project/architecture, /services moved: redirect to the project with that view
//   /projects/$project/settings                Settings: the project's details, adopted or not,
//                                              and where its alerts go
//   /projects/$project/objects                 Objects: every object by kind, ?object=… opens
//                                              its YAML
//   /projects/$project/create                  objects written as YAML, created through a plan
//   /projects/$project/terminal                moved: the project, with its kubectl terminal in
//                                              the dock
//   /projects/$project/services/$kind/$name    one service: ?tab=logs|deployments|networking|
//                                              variables|settings (Overview when absent), or a
//                                              view from its menu (events, yaml); a former
//                                              terminal view opens Logs with the dock;
//                                              Settings' parts as #anchors; a former ?tab=status…
//                                              redirects to its place
//   /projects/$project/databases/$engine/$name one database an operator runs: ?tab=logs|backups|
//                                              events|settings (Overview when absent), and
//                                              Settings' YAML view (?mode=yaml)
//   /settings                                  preferences, the signed-in person, About
//   /account                                   your account: who you are, your role in each
//                                              project, a link to manage how you sign in
//   /members                                   everyone Zeabur Identity signs in: invite, setup
//                                              links, reset, remove
//   /settings/components                       moved: redirects to /cluster/components
//   /problems/$kind                            what one kind of problem means, and what to do
//
// Data. Each route's loader asks TanStack Query for what its page reads, so a page opens with its
// data instead of a loading state (and hovering a link loads it ahead, `defaultPreload`). Query
// owns the cache: the router keeps no copy (`defaultPreloadStaleTime: 0`) and a loader never
// fails on an answer from the cluster. A forbidden or missing project is the page's to show, in
// place, with the rest of the page around it; the loader only waits for it.
//
// State. A page's own state (a revealed Secret, the log filter, the canvas's pan and zoom) belongs
// to one project or service: `remountDeps` gives the page a fresh instance when the project or the
// service changes, so nothing carries over to the next one.

/** A query's data, from the cache when it is there. */
const ensure: typeof queryClient.ensureQueryData = (options) => queryClient.ensureQueryData(options)

/** Waits for every read, or for the first that finds the cluster away; a failure is left for the
 * page to show. */
const load = (...reads: Promise<unknown>[]) =>
  new Promise<void>((resolve) => {
    let left = reads.length
    if (left === 0) resolve()
    for (const read of reads)
      read.then(
        () => --left === 0 && resolve(),
        (error: unknown) => (isUnreachable(error) || --left === 0) && resolve(),
      )
  })

/** The project document and its newest events, which every project page reads. */
const loadProject = (project: string) =>
  load(ensure(projectQuery(project)), ensure(eventsQuery(project)), ensure(namespaceQuery(project)))

const rootRoute = createRootRoute({
  component: AppShell,
  notFoundComponent: NotFound,
})

// Home: what is running and what is broken across projects. The cluster's machines have their
// own page; one cluster for now, and `/cluster` leaves room for a list of them.
const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  validateSearch: validateProjectsSearch,
  loader: () =>
    load(
      ensure(projectsQuery),
      ensure(clusterQuery),
      ensure(nodesQuery),
      ensure(upgradesQuery),
      ensure(recentWarningsQuery),
    ),
  component: lazyRouteComponent(() => import("@/routes/home"), "HomePage"),
})

const clusterRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/cluster",
  loader: () =>
    load(
      ensure(clusterQuery),
      ensure(upgradesQuery),
      ensure(nodesQuery),
      ensure(clusterWarningsQuery),
    ),
  component: lazyRouteComponent(() => import("@/routes/cluster/page"), "ClusterPage"),
})

const projectsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects",
  validateSearch: validateProjectsSearch,
  loader: () => load(ensure(projectsQuery)),
  component: lazyRouteComponent(() => import("@/routes/projects"), "ProjectsPage"),
})

const projectRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project",
  validateSearch: validateProjectSearch,
  // Incidents and suggestions add to the header and the top of the page, so they are read first.
  loader: ({ params }) =>
    load(
      loadProject(params.project),
      ensure(projectIncidentsQuery(params.project)),
      ensure(projectGapsQuery(params.project)),
    ),
  remountDeps: ({ params }) => params.project,
  component: lazyRouteComponent(() => import("@/routes/project/page"), "ProjectPage"),
})

// Architecture and Services were pages of their own; old links still arrive.
const architectureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/architecture",
  validateSearch: validateProjectSearch,
  beforeLoad: ({ params, search }) => {
    throw redirect({
      to: "/projects/$project",
      params,
      search: { ...search, view: "map" },
      replace: true,
    })
  },
})

const servicesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/services",
  validateSearch: validateProjectSearch,
  beforeLoad: ({ params, search }) => {
    throw redirect({
      to: "/projects/$project",
      params,
      search: { ...search, view: "list" },
      replace: true,
    })
  },
})

// Objects reads a dozen or so kinds when it opens and more on demand, so its loader does not wait
// for them: the page shows its own loading state, and reads only while it is open.
const objectsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/objects",
  validateSearch: validateObjectsSearch,
  remountDeps: ({ params }) => params.project,
  component: lazyRouteComponent(() => import("@/routes/project/objects"), "ObjectsPage"),
})

const createObjectsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/create",
  remountDeps: ({ params }) => params.project,
  component: lazyRouteComponent(() => import("@/routes/project/create"), "CreatePage"),
})

const projectSettingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/settings",
  loader: ({ params }) => load(ensure(namespaceQuery(params.project))),
  remountDeps: ({ params }) => params.project,
  component: lazyRouteComponent(() => import("@/routes/project/settings"), "ProjectSettingsPage"),
})

// The kubectl terminal was a page; it is a tab of the dock under every page now.
const toolsTerminalRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/terminal",
  beforeLoad: async ({ params, preload }) => {
    if (!preload) {
      // Where Zeabur runs on the person's computer there is no kubectl terminal: the dock opens empty.
      const tools = await ensure(toolsQuery(params.project)).catch(() => undefined)
      if (tools?.available) openKubectl(params.project, { ask: false })
      else showDock(true)
    }
    throw redirect({ to: "/projects/$project", params, replace: true })
  },
})

const serviceRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/services/$kind/$name",
  validateSearch: validateServiceSearch,
  // `deployment/worker`, as kubectl prints it, opens `Deployment/worker`.
  beforeLoad: ({ params, search, location, preload }) => {
    const kind = canonicalKind(params.kind)
    const former = search.former
    if (kind !== params.kind || former) {
      const place = former ? formerPlace(former, search.view, location.hash) : undefined
      if (place?.terminal && !preload)
        openShell(params.project, { kind, name: params.name }, search.pod)
      throw redirect({
        to: "/projects/$project/services/$kind/$name",
        params: { ...params, kind },
        search: {
          ...search,
          tab: place ? place.tab : search.tab,
          mode: place ? place.mode : search.mode,
          former: undefined,
          view: undefined,
        },
        hash: place ? place.hash : location.hash,
        replace: true,
      })
    }
  },
  // The rollout and the cluster's name are in the status at the top, so they are read first.
  loader: ({ params }) =>
    load(
      loadProject(params.project),
      ensure(clusterQuery),
      ...(isTask(params.kind)
        ? []
        : [ensure(rolloutQuery(params.project, params.kind, params.name))]),
    ),
  remountDeps: ({ params }) => `${params.project}/${params.kind}/${params.name}`,
  component: lazyRouteComponent(() => import("@/routes/service/page"), "ServicePage"),
})

const databaseRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/projects/$project/databases/$engine/$name",
  validateSearch: validateDatabaseSearch,
  loader: ({ params }) => load(ensure(databasesQuery(params.project))),
  remountDeps: ({ params }) => `${params.project}/${params.engine}/${params.name}`,
  component: lazyRouteComponent(() => import("@/routes/database/page"), "DatabasePage"),
})

const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings",
  component: lazyRouteComponent(() => import("@/routes/settings"), "SettingsPage"),
})

const accountRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/account",
  loader: () => load(ensure(accountQuery)),
  component: lazyRouteComponent(() => import("@/routes/account"), "AccountPage"),
})

const membersRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/members",
  loader: () => load(ensure(membersQuery)),
  component: lazyRouteComponent(() => import("@/routes/members"), "MembersPage"),
})

const clusterBackupsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/cluster/backups",
  loader: () => load(ensure(clusterQuery), ensure(upgradesQuery), ensure(clusterBackupsQuery)),
  component: lazyRouteComponent(() => import("@/routes/cluster/backups"), "BackupsPage"),
})

const clusterNotificationsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/cluster/notifications",
  loader: () => load(ensure(clusterQuery), ensure(upgradesQuery), ensure(notificationsQuery())),
  component: lazyRouteComponent(
    () => import("@/routes/cluster/notifications"),
    "NotificationsPage",
  ),
})

const componentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/cluster/components",
  validateSearch: validateComponentsSearch,
  loader: () => load(ensure(clusterQuery), ensure(upgradesQuery), ensure(capabilitiesQuery)),
  component: lazyRouteComponent(() => import("@/routes/cluster/components"), "ComponentsPage"),
})

// What each problem type means: its `type` URI is this page (RFC 9457, section 3.1.1).
const problemRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/problems/$kind",
  component: lazyRouteComponent(() => import("@/routes/problem"), "ProblemPage"),
})

// Components used to be a settings page; old links still arrive.
const movedComponentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/settings/components",
  beforeLoad: () => {
    throw redirect({ to: "/cluster/components", replace: true })
  },
})

export const router = createRouter({
  routeTree: rootRoute.addChildren([
    homeRoute,
    clusterRoute,
    projectsRoute,
    projectRoute,
    architectureRoute,
    servicesRoute,
    objectsRoute,
    createObjectsRoute,
    projectSettingsRoute,
    toolsTerminalRoute,
    serviceRoute,
    databaseRoute,
    settingsRoute,
    accountRoute,
    membersRoute,
    clusterBackupsRoute,
    clusterNotificationsRoute,
    componentsRoute,
    problemRoute,
    movedComponentsRoute,
  ]),
  defaultPreload: "intent",
  defaultPreloadStaleTime: 0,
  defaultViewTransition: viewTransitionTypes() ? pageViewTransition : false,
  defaultPendingComponent: RoutePending,
  defaultErrorComponent: RouteError,
  scrollRestoration: true,
})

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router
  }
}
