import { useEffect, useRef, useState } from "react"
import ArrowSquareOutIcon from "~icons/ph/arrow-square-out"
import CaretRightIcon from "~icons/ph/caret-right"
import DownloadIcon from "~icons/ph/download-simple"
import WarningCircleIcon from "~icons/ph/warning-circle"

import type { Plan } from "@/api"
import type { Wizard } from "@/app"
import { Code, Panel, Row, Rows, StatusPill } from "@/components/bits"
import { useT, type Translate } from "@/i18n"
import { StepPage } from "@/steps/frame"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Button, buttonVariants } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"
import { CopyInput } from "@zeabur/design-system/components/copy-input"
import { Timeline, TimelineItem } from "@zeabur/design-system/components/timeline"

interface Phase {
  title: string
  /** The `==> ` line `apply` prints when it starts this phase. */
  starts: (step: string) => boolean
}

/** What `apply` does, in its order (crates/zeabur-installer/src/apply.rs). */
function phases(t: Translate, plan: Plan | undefined, local: boolean): Phase[] {
  const rke2 = plan?.rke2.version ?? ""
  return [
    { title: t("phase.host"), starts: (s) => s === "Checking the host" },
    { title: t("phase.rke2", rke2), starts: (s) => s.startsWith("RKE2 ") },
    { title: t("phase.namespaces"), starts: (s) => s === "Namespaces and network policies" },
    { title: t("phase.storage"), starts: (s) => s === "Storage" },
    ...(plan?.components ?? []).map((c) => ({
      title: c.displayName,
      starts: (s: string) => s === `${c.displayName} (${c.id})`,
    })),
    ...(local
      ? [{ title: t("phase.ca"), starts: (s: string) => s === "Local certificate authority" }]
      : []),
    { title: t("phase.identity"), starts: (s) => s === "Zeabur Identity" },
    { title: t("phase.zeabur"), starts: (s) => s === "Zeabur" },
    { title: t("phase.done"), starts: (s) => s === "Done" },
  ]
}

const foldAbove = 7

type Cause = "memory" | "host" | "timeout" | "other"

/** What the error line says went wrong, where it is recognisable. */
function causeOf(line: string): Cause {
  if (/insufficient memory|out of memory|oomkilled|cannot allocate memory/i.test(line))
    return "memory"
  if (line.includes("the host is not ready")) return "host"
  if (/timed out/i.test(line)) return "timeout"
  return "other"
}

/** The line after "Enroll the first admin's passkey" (apply.rs, `setup_link`). */
function setupLink(lines: string[]): string | null {
  const at = lines.findIndex((line) => line.startsWith("Enroll the first admin"))
  return at >= 0 ? (lines[at + 1]?.trim() ?? null) : null
}

export function InstallStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { progress, config, planned } = wizard
  const plan = planned && "plan" in planned ? planned.plan : undefined
  const local = config.access.mode === "local"
  const all = phases(t, plan, local)
  const started = progress.lines
    .filter((line) => line.startsWith("==> "))
    .map((line) => all.findIndex((phase) => phase.starts(line.slice(4))))
    .filter((i) => i >= 0)
  const current = started.length ? Math.max(...started) : 0
  const done = progress.code === 0
  const failed = progress.code !== null && progress.code !== 0
  const link = setupLink(progress.lines)
  const zeabur = plan?.zeabur?.url ?? `https://${config.access.zeabur}`
  const identity = plan?.identity?.issuer ?? `https://${config.access.identity}`

  const [expanded, setExpanded] = useState(false)
  const at = all[current]?.title ?? ""
  const error =
    progress.lines.findLast((line) => line.startsWith("error: ")) ?? progress.lines.at(-1)
  const cause = failed ? causeOf(error ?? "") : null
  const doneCount = done ? all.length : current
  const waitingCount = done ? 0 : all.length - current - 1
  // Long timelines show the step at hand with its neighbours; the rest fold into counts.
  const fold = !expanded && all.length > foldAbove
  const first = done ? all.length : Math.max(0, current - 1)
  const last = done ? all.length - 1 : Math.min(all.length - 1, current + 1)
  const shown = all
    .map((phase, i) => ({ phase, i }))
    .filter(({ i }) => !fold || (i >= first && i <= last))

  const tone = (i: number) =>
    done || i < current ? "ok" : i > current ? "neutral" : failed ? "fail" : "progress"
  const word = (i: number) =>
    done || i < current
      ? t("install.done")
      : i > current
        ? t("install.waiting")
        : failed
          ? t("install.stopped")
          : t("install.running")

  return (
    <StepPage
      title={done ? t("done.title") : failed ? t("install.failedTitle") : t("install.title")}
      description={
        done ? t("done.description") : failed ? t("install.failed") : t("install.description")
      }
      badges={
        done ? (
          <StatusPill health="ok">{t("install.done")}</StatusPill>
        ) : failed ? (
          <StatusPill health="fail">{t("install.stoppedAt", at)}</StatusPill>
        ) : (
          <StatusPill health="progress">{t("install.running")}</StatusPill>
        )
      }
      back={
        failed ? (
          <Button variant="outline" onClick={() => wizard.go("review")}>
            {t("common.back")}
          </Button>
        ) : undefined
      }
      next={failed ? <Button onClick={wizard.install}>{t("install.again")}</Button> : undefined}
    >
      {cause ? (
        <Alert variant="destructive">
          <WarningCircleIcon aria-hidden="true" />
          <AlertTitle>
            {cause === "memory"
              ? t("install.cause.memory", at)
              : cause === "host"
                ? t("install.cause.host")
                : cause === "timeout"
                  ? t("install.cause.timeout", at)
                  : t("install.cause.other", at)}
          </AlertTitle>
          <AlertDescription>
            <p>
              {cause === "memory"
                ? t("install.next.memory")
                : cause === "host"
                  ? t("install.next.host")
                  : t("install.next.log")}
            </p>
            {cause === "memory" || cause === "host" ? (
              <div className="mt-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => {
                    if (cause === "host") wizard.reload()
                    wizard.go(cause === "host" ? "host" : "components")
                  }}
                >
                  {cause === "host" ? t("review.backToChecks") : t("install.toComponents")}
                </Button>
              </div>
            ) : null}
          </AlertDescription>
        </Alert>
      ) : null}

      {done ? (
        <>
          <Panel title={t("done.setup")}>
            {link ? (
              <div className="flex flex-col gap-3">
                <p className="text-sm text-secondary">{t("done.setupHint")}</p>
                <div className="flex flex-col gap-2 sm:flex-row">
                  <CopyInput value={link} copyLabel={t("common.copy")} className="min-w-0 flex-1" />
                  <a
                    href={link}
                    target="_blank"
                    rel="noreferrer"
                    className={buttonVariants({ variant: "primary" })}
                  >
                    {t("done.register")}
                    <ArrowSquareOutIcon aria-hidden="true" />
                  </a>
                </div>
              </div>
            ) : (
              <p className="text-sm text-secondary">{t("done.noLink")}</p>
            )}
          </Panel>
          <Panel
            title={t("done.zeabur")}
            action={
              <a
                href={zeabur}
                target="_blank"
                rel="noreferrer"
                className={buttonVariants({ variant: "outline", size: "sm" })}
              >
                {t("done.open")}
                <ArrowSquareOutIcon aria-hidden="true" />
              </a>
            }
          >
            <Rows>
              <Row label={t("done.zeabur")}>
                <span className="font-mono text-xs">{zeabur}</span>
              </Row>
              <Row label={t("done.identity")}>
                <span className="font-mono text-xs">{identity}</span>
              </Row>
              {local ? (
                <Row label={t("done.ca")}>
                  <div className="flex flex-col items-start gap-2">
                    <span className="text-sm text-secondary">{t("done.caHint")}</span>
                    <a
                      href="/api/local-ca.crt"
                      className={buttonVariants({ variant: "outline", size: "sm" })}
                    >
                      <DownloadIcon aria-hidden="true" />
                      {t("done.caDownload")}
                    </a>
                  </div>
                </Row>
              ) : null}
            </Rows>
          </Panel>
        </>
      ) : null}

      <Panel
        title={t("step.install")}
        action={
          <span className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-secondary">
            <span>
              {[
                t("install.doneCount", doneCount),
                done ? "" : failed ? t("install.stoppedAt", at) : t("install.runningAt", at),
                waitingCount ? t("install.waitingCount", waitingCount) : "",
              ]
                .filter(Boolean)
                .join(" · ")}
            </span>
            {all.length > foldAbove ? (
              <Button variant="ghost" size="xs" onClick={() => setExpanded(!expanded)}>
                {expanded ? t("install.showFewer") : t("install.showAll", all.length)}
              </Button>
            ) : null}
          </span>
        }
      >
        <Timeline>
          {fold && first > 0 ? (
            <TimelineItem tone="ok" title={t("install.foldDone", first)} />
          ) : null}
          {shown.map(({ phase, i }) => (
            <TimelineItem key={phase.title} tone={tone(i)} title={phase.title} time={word(i)} />
          ))}
          {fold && last < all.length - 1 ? (
            <TimelineItem tone="neutral" title={t("install.foldWaiting", all.length - 1 - last)} />
          ) : null}
        </Timeline>
      </Panel>

      <Log key={failed ? "failed" : "running"} lines={progress.lines} defaultOpen={failed} />
    </StepPage>
  )
}

function Log({ lines, defaultOpen }: { lines: string[]; defaultOpen: boolean }) {
  const t = useT()
  const [open, setOpen] = useState(defaultOpen)
  const box = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (open && box.current) box.current.scrollTop = box.current.scrollHeight
  }, [open, lines.length])
  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <CollapsibleTrigger variant="disclosure">
        <CaretRightIcon aria-hidden="true" />
        {open ? t("install.hideLog") : t("install.showLog")}
      </CollapsibleTrigger>
      <CollapsibleContent>
        <div ref={box} className="mt-3 max-h-log overflow-y-auto rounded-lg">
          <Code>{lines.join("\n")}</Code>
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}
