import { useEffect, useState } from "react"
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
import { AccessStep } from "@/steps/access"
import { ComponentsStep } from "@/steps/components"
import { HostStep } from "@/steps/host"
import { InstallStep } from "@/steps/install"
import { ReviewStep } from "@/steps/review"
import { setTheme, useTheme, type ThemeChoice } from "@/theme"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Logo } from "@zeabur/design-system/components/brand"
import { Button } from "@zeabur/design-system/components/button"
import { PageColumn } from "@zeabur/design-system/components/page-column"
import { Select } from "@zeabur/design-system/components/select"
import { LoadingDots } from "@zeabur/design-system/components/spinner"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

const steps = ["host", "access", "components", "review", "install"] as const
export type Step = (typeof steps)[number]
/** A personal install has no Zeabur panel, so nothing to reach it by. */
const personalSteps: readonly Step[] = steps.filter((s) => s !== "access")

/** The memory below which a team install is refused, when the server does not say
 * (`Distribution::min_memory_mib`). */
const teamMinMemoryMib: Record<Distribution, number> = { k3s: 2400, rke2: 2600 }
/** From this much memory RKE2 is suggested (`RKE2_SUGGESTED_MIB`). */
const rke2SuggestedMib = 7 * 1024

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

const idle: Progress = { lines: [], next: 0, running: false, code: null }

export function App() {
  const t = useT()
  const [server, setServer] = useState<ServerState | null>(null)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [config, setConfigState] = useState<Config | null>(null)
  const [personal, setPersonal] = useState<PersonalConfig>(emptyPersonal)
  const [use, setUseState] = useState<Use | null>(null)
  const [secrets, setSecrets] = useState<Secrets>({})
  const [planned, setPlanned] = useState<Planned | null>(null)
  const [shown, setShown] = useState<Plan | null>(null)
  const [planning, setPlanning] = useState(false)
  const [step, setStep] = useState<Step>("host")
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
        const teamFits = fits(state, "k3s") || fits(state, "rke2")
        setUseState(
          (current) => current ?? (state.use === "personal" || !teamFits ? "personal" : "team"),
        )
        if (state.applying) {
          setStep("install")
          setProgress({ ...idle, running: true })
        }
      },
      (error: unknown) => setLoadError(String(error)),
    )
  }
  useEffect(load, [])

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

  const go = (next: Step) => {
    setStep(next)
    window.scrollTo(0, 0)
  }

  const install = () => {
    if (!active) return
    api.apply(active, secrets).then(
      (result) => {
        if ("error" in result) {
          setPlanned({ error: result.error })
          return
        }
        setProgress({ ...idle, running: true })
        go("install")
      },
      (error: unknown) => setPlanned({ error: String(error) }),
    )
  }

  const teamFits = fits(server, "k3s") || fits(server, "rke2")
  const shownSteps = use === "personal" ? personalSteps : steps
  const wizard: Wizard | null =
    server && config && use && active
      ? {
          server,
          use,
          teamFits,
          suggested: suggested(server),
          setUse: (next) => {
            if (next === "team" && !teamFits) return
            setUseState(next)
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
            label={t("step.of", shownSteps.indexOf(step) + 1, shownSteps.length)}
            steps={shownSteps.map((s) => t(`step.${s}`))}
            current={shownSteps.indexOf(step)}
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
          ) : step === "host" ? (
            <HostStep wizard={wizard} />
          ) : step === "access" ? (
            <AccessStep wizard={wizard} />
          ) : step === "components" ? (
            <ComponentsStep wizard={wizard} />
          ) : step === "review" ? (
            <ReviewStep wizard={wizard} />
          ) : (
            <InstallStep wizard={wizard} />
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
