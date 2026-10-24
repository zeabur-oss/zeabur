import { useEffect, useState } from "react"
import DesktopIcon from "~icons/ph/desktop"
import MoonIcon from "~icons/ph/moon"
import SunIcon from "~icons/ph/sun"

import {
  api,
  type Config,
  type Plan,
  type Planned,
  type Progress,
  type Secrets,
  type ServerState,
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

export interface Wizard {
  server: ServerState
  config: Config
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
        setConfigState((current) => current ?? state.config)
        if (state.applying) {
          setStep("install")
          setProgress({ ...idle, running: true })
        }
      },
      (error: unknown) => setLoadError(String(error)),
    )
  }
  useEffect(load, [])

  // Every change is planned by the server, so the pages show exactly what `apply` would do.
  useEffect(() => {
    if (!config || !server?.catalog) return
    let current = true
    setPlanning(true)
    const timer = setTimeout(() => {
      api.plan(config, secrets).then(
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
  }, [config, secrets, server?.catalog])

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
    if (!config) return
    api.apply(config, secrets).then(
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

  const wizard: Wizard | null =
    server && config
      ? {
          server,
          config,
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
            label={t("step.of", steps.indexOf(step) + 1, steps.length)}
            steps={steps.map((s) => t(`step.${s}`))}
            current={steps.indexOf(step)}
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
