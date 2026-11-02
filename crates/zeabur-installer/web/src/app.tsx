import { Navigate, Outlet, useLocation, useNavigate, useSearch } from "@tanstack/react-router"
import { createContext, use as useContext, useEffect, useState } from "react"
import DesktopIcon from "~icons/ph/desktop"
import MoonIcon from "~icons/ph/moon"
import SunIcon from "~icons/ph/sun"

import {
  api,
  type Config,
  type Distribution,
  type PersonalConfig,
  type Plan,
  type Planned,
  type Progress,
  type Secrets,
  type ServerState,
  type Use,
} from "@/api"
import { Steps } from "@/components/bits"
import { localeNames, locales, useI18n, useT } from "@/i18n"
import { setTheme, useTheme, type ThemeChoice } from "@/theme"
import { Alert, AlertDescription, AlertTitle } from "@zeabur-oss/design-system/components/alert"
import { Logo } from "@zeabur-oss/design-system/components/brand"
import { Button } from "@zeabur-oss/design-system/components/button"
import { PageColumn } from "@zeabur-oss/design-system/components/page-column"
import { Select } from "@zeabur-oss/design-system/components/select"
import { LoadingDots } from "@zeabur-oss/design-system/components/spinner"
import { Tooltip } from "@zeabur-oss/design-system/components/tooltip"

const steps = ["host", "access", "components", "settings", "review", "install"] as const
export type Step = (typeof steps)[number]
/** A personal install has no Zeabur panel, so nothing to reach it by. */
const personalSteps: readonly Step[] = steps.filter((s) => s !== "access")

/** The memory below which a team install is refused, when the server does not say
 * (`Distribution::min_memory_mib`). */
const teamMinMemoryMib: Record<Distribution, number> = { k3s: 2400, rke2: 2600 }
/** From this much memory RKE2 is suggested (`RKE2_SUGGESTED_MIB`). */
const rke2SuggestedMib = 7 * 1024

/** The page of each step; the kind of install is `?use=`. */
export const stepPath = (step: Step) => (step === "host" ? "/" : (`/${step}` as const))

const memoryOf = (server: ServerState | null) => server?.facts?.memoryMib ?? Infinity

/** Whether a team install on the distribution fits the server's memory. */
export const fits = (server: ServerState | null, distribution: Distribution) =>
  memoryOf(server) >= (server?.teamMinMemoryMib?.[distribution] ?? teamMinMemoryMib[distribution])

/** The distribution to suggest for the server's memory. */
export const suggested = (server: ServerState | null): Distribution =>
  memoryOf(server) >= (server?.rke2SuggestedMib ?? rke2SuggestedMib) ? "rke2" : "k3s"

const emptyPersonal: PersonalConfig = {
  use: "personal",
  enable: [],
  disable: [],
  inputs: {},
  secretFiles: {},
  host: { tlsSan: [], requireEncryptedDisk: false },
  sources: {},
}

/** The choices the components page edits, in either kind of configuration. */
export type Choices = Pick<Config, "enable" | "disable" | "inputs" | "allowDataLoss">

export interface Wizard {
  server: ServerState
  use: Use
  /** Whether a team install fits this server's memory. */
  teamFits: boolean
  /** The distribution suggested for this server. */
  suggested: Distribution
  setUse: (use: Use) => void
  config: Config
  personal: PersonalConfig
  /** The configuration of the chosen kind, as `apply` gets it. */
  active: Config | PersonalConfig
  setChoices: (choices: Choices) => void
  /** Sets `host.nodeName` in the configuration of the chosen kind. */
  setNodeName: (name: string) => void
  /** Joins or leaves the experience program in the configuration of the chosen kind. */
  setTelemetry: (on: boolean) => void
  steps: readonly Step[]
  secrets: Secrets
  planned: Planned | null
  /** The last plan that succeeded, so choices stay visible while a newer one fails. */
  shown: Plan | null
  planning: boolean
  progress: Progress
  setConfig: (config: Config) => void
  setSecret: (component: string, input: string, value: string) => void
  go: (step: Step) => void
  reload: () => void
  install: () => void
}

const WizardContext = createContext<Wizard | null>(null)

/** The wizard of the page around the step; steps render only once it has loaded. */
export const useWizard = () => useContext(WizardContext)!

/** The choices so far, kept for this tab so a reload does not lose them. Secrets are left out
 * and asked again. */
const saved = "zeabur-installer.choices"
interface Saved {
  config: Config | null
  personal: PersonalConfig
}

const restore = (): Saved | null => {
  try {
    const value = window.sessionStorage.getItem(saved)
    return value ? (JSON.parse(value) as Saved) : null
  } catch {
    return null
  }
}

const keep = (choices: Saved | null) => {
  try {
    if (choices) window.sessionStorage.setItem(saved, JSON.stringify(choices))
    else window.sessionStorage.removeItem(saved)
  } catch {
    // A page without storage starts over on reload.
  }
}

const idle: Progress = { lines: [], next: 0, running: false, code: null }

/** Everything the steps share, around the page of the current step. */
export function WizardPage() {
  const t = useT()
  const navigate = useNavigate()
  const pathname = useLocation({ select: (l) => l.pathname })
  const linkedUse = useSearch({ strict: false, select: (s) => s.use })
  const [server, setServer] = useState<ServerState | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [restored] = useState(restore)
  const [config, setConfigState] = useState<Config | null>(restored?.config ?? null)
  const [personal, setPersonal] = useState<PersonalConfig>(restored?.personal ?? emptyPersonal)
  const [secrets, setSecrets] = useState<Secrets>({})
  const [planned, setPlanned] = useState<Planned | null>(null)
  const [shown, setShown] = useState<Plan | null>(null)
  const [planning, setPlanning] = useState(false)
  const [progress, setProgress] = useState<Progress>(idle)

  const load = () => {
    setLoadError(null)
    api.state().then(
      (state) => {
        setServer(state)
        setConfigState(
          (current) =>
            current ??
            (state.config && !state.configExists
              ? { ...state.config, distribution: suggested(state) }
              : state.config),
        )
        if (state.personal)
          setPersonal((current) => (current === emptyPersonal ? state.personal! : current))
        if (state.applying) {
          setProgress({ ...idle, running: true })
          void navigate({ to: "/install", search: (s) => s, replace: true })
        }
      },
      (error: unknown) => setLoadError(String(error)),
    )
  }
  useEffect(load, [])

  const teamFits = fits(server, "k3s") || fits(server, "rke2")
  const use: Use | null = !server
    ? null
    : linkedUse === "personal" || !teamFits
      ? "personal"
      : (linkedUse ?? (server.use === "personal" ? "personal" : "team"))
  const active = use === "personal" ? personal : config

  // Every change is planned by the server, so the pages show exactly what `apply` would do.
  useEffect(() => {
    if (!active || !server?.catalog) return
    let current = true
    setPlanning(true)
    const timer = setTimeout(() => {
      api.plan(active, secrets).then(
        (next) => {
          if (!current) return
          setPlanned(next)
          if ("plan" in next) setShown(next.plan)
          setPlanning(false)
        },
        (error: unknown) => {
          if (!current) return
          setPlanned({ error: String(error) })
          setPlanning(false)
        },
      )
    }, 250)
    return () => {
      current = false
      clearTimeout(timer)
    }
  }, [active, secrets, server?.catalog])

  useEffect(() => {
    if (config) keep({ config, personal })
  }, [config, personal])

  // The log, read while the install runs.
  useEffect(() => {
    if (!progress.running) return
    const timer = setTimeout(() => {
      api.progress(progress.next).then(
        (next) => setProgress((before) => ({ ...next, lines: [...before.lines, ...next.lines] })),
        () => setProgress((before) => ({ ...before })),
      )
    }, 1000)
    return () => clearTimeout(timer)
  }, [progress])

  const go = (next: Step) => void navigate({ to: stepPath(next), search: (s) => s })

  const install = () => {
    if (!active) return
    api.apply(active, secrets).then(
      (result) => {
        if ("error" in result) {
          setPlanned({ error: result.error })
          return
        }
        keep(null)
        setProgress({ ...idle, running: true })
        go("install")
      },
      (error: unknown) => setPlanned({ error: String(error) }),
    )
  }

  const shownSteps = use === "personal" ? personalSteps : steps
  // A linked install page shows nothing until an install has started.
  const started = progress.running || progress.code !== null
  const step = steps.find((s) => stepPath(s) === pathname) ?? "host"
  const current = !shownSteps.includes(step)
    ? "host"
    : step === "install" && !started
      ? "review"
      : step
  const wizard: Wizard | null =
    server && config && use && active
      ? {
          server,
          use,
          teamFits,
          suggested: suggested(server),
          setUse: (next) => {
            if (next === "team" && !teamFits) return
            void navigate({ to: ".", search: (s) => ({ ...s, use: next }), replace: true })
            setShown(null)
          },
          config,
          personal,
          active,
          setChoices: (choices) =>
            use === "personal"
              ? setPersonal({ ...personal, ...choices })
              : setConfigState({ ...config, ...choices }),
          setNodeName: (nodeName) =>
            use === "personal"
              ? setPersonal({ ...personal, host: { ...personal.host, nodeName } })
              : setConfigState({ ...config, host: { ...config.host, nodeName } }),
          setTelemetry: (telemetry) =>
            use === "personal"
              ? setPersonal({ ...personal, telemetry })
              : setConfigState({ ...config, telemetry }),
          steps: shownSteps,
          secrets,
          planned,
          shown,
          planning,
          progress,
          setConfig: setConfigState,
          setSecret: (component, input, value) =>
            setSecrets((before) => ({
              ...before,
              [component]: { ...before[component], [input]: value },
            })),
          go,
          reload: load,
          install,
        }
      : null

  return (
    <div className="flex min-h-dvh flex-col bg-surface-page">
      <TopBar />
      <main className="flex-1 px-4 md:px-8">
        <PageColumn width="content" gap="md" className="my-6 md:my-10">
          <Steps
            label={t("step.of", shownSteps.indexOf(current) + 1, shownSteps.length)}
            steps={shownSteps.map((s) => t(`step.${s}`))}
            current={shownSteps.indexOf(current)}
          />
          {loadError ? (
            <Alert variant="destructive">
              <AlertTitle>{t("common.error")}</AlertTitle>
              <AlertDescription>{loadError}</AlertDescription>
            </Alert>
          ) : !wizard ? (
            <div className="flex items-center gap-2 text-sm text-secondary">
              <LoadingDots />
              {t("common.loading")}
            </div>
          ) : current !== step ? (
            <Navigate to={stepPath(current)} search={(s) => s} replace />
          ) : (
            <WizardContext value={wizard}>
              <Outlet />
            </WizardContext>
          )}
        </PageColumn>
      </main>
    </div>
  )
}

function TopBar() {
  const t = useT()
  const { locale, setLocale } = useI18n()
  const theme = useTheme()
  const themes: Record<ThemeChoice, typeof SunIcon> = {
    system: DesktopIcon,
    light: SunIcon,
    dark: MoonIcon,
  }
  const ThemeIcon = themes[theme]
  const nextTheme: ThemeChoice =
    theme === "system" ? "light" : theme === "light" ? "dark" : "system"
  return (
    <header className="flex h-14 shrink-0 items-center gap-3 border-b border-main bg-card px-4 md:px-6">
      <Logo className="h-5" aria-label="Zeabur" />
      <span className="text-sm whitespace-nowrap text-secondary">{t("app.name")}</span>
      <div className="ml-auto flex items-center gap-1.5">
        <Select
          aria-label={t("app.language")}
          size="sm"
          value={locale}
          onValueChange={setLocale}
          items={locales.map((value) => ({ value, label: localeNames[value], lang: value }))}
        />
        <Tooltip content={t(`theme.${theme}`)}>
          <Button
            variant="ghost"
            size="icon-sm"
            aria-label={t("app.theme")}
            onClick={() => setTheme(nextTheme)}
          >
            <ThemeIcon aria-hidden="true" />
          </Button>
        </Tooltip>
      </div>
    </header>
  )
}
