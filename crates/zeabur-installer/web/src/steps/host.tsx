import ArrowClockwiseIcon from "~icons/ph/arrow-clockwise"
import CaretRightIcon from "~icons/ph/caret-right"

import type { Check } from "@/api"
import type { Wizard } from "@/app"
import { Panel, Row, Rows, StatusDot, StatusPill, type Health } from "@/components/bits"
import { coded, useT } from "@/i18n"
import { StepPage } from "@/steps/frame"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@zeabur/design-system/components/collapsible"

const health: Record<string, Health> = { pass: "ok", warn: "warn", fail: "fail" }
const order: Record<Check["status"], number> = { fail: 0, warn: 1, pass: 2 }
const checkKey = (check: Check) => [check.name, check.code, ...check.values].join(" ")

export function CheckRow({ check }: { check: Check }) {
  const t = useT()
  return (
    <div className="flex items-start gap-3 px-5 py-3">
      <StatusDot health={health[check.status] ?? "idle"} className="mt-1.5" />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <span className="text-sm font-medium text-main">
          {coded(t, `check.${check.name}`, [], check.name)}
        </span>
        <span className="text-sm text-secondary">
          {coded(t, `code.${check.code}`, check.values, check.detail)}
        </span>
      </div>
    </div>
  )
}

export function HostStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { server } = wizard
  const facts = server.facts
  const failed = server.checks.filter((c) => c.status === "fail").length
  const warned = server.checks.filter((c) => c.status === "warn").length
  const exceptions = server.checks
    .filter((c) => c.status !== "pass")
    .sort((a, b) => order[a.status] - order[b.status])
  const passed = server.checks.filter((c) => c.status === "pass")
  return (
    <StepPage
      title={t("host.title")}
      description={t("host.description")}
      badges={
        failed ? (
          <StatusPill health="fail">{t("host.failed", failed)}</StatusPill>
        ) : warned ? (
          <StatusPill health="warn">{t("host.warned", warned)}</StatusPill>
        ) : (
          <StatusPill health="ok">{t("host.passed")}</StatusPill>
        )
      }
      next={
        <>
          <Button variant="outline" onClick={wizard.reload}>
            <ArrowClockwiseIcon aria-hidden="true" />
            {t("host.recheck")}
          </Button>
          <Button disabled={!server.catalog} onClick={() => wizard.go("access")}>
            {t("common.continue")}
          </Button>
        </>
      }
    >
      <Panel title={t("host.server")}>
        <Rows>
          <Row label={t("host.name")}>
            <span translate="no">{facts?.hostname ?? "—"}</span>
          </Row>
          <Row label={t("host.arch")}>
            <span className="font-mono text-xs">{facts?.arch ?? "—"}</span>
          </Row>
          <Row label={t("host.memory")}>
            {facts ? t("common.gib", (facts.memoryMib / 1024).toFixed(1)) : "—"}
          </Row>
          <Row label={t("host.addresses")}>
            <span className="flex flex-wrap gap-x-3 gap-y-1 font-mono text-xs">
              {server.addresses.length
                ? server.addresses.map((address) => <span key={address}>{address}</span>)
                : "—"}
            </span>
          </Row>
        </Rows>
      </Panel>

      <Panel
        title={t("host.checks")}
        flush
        action={
          exceptions.length ? (
            <span className="text-xs text-secondary">{t("host.passedCount", passed.length)}</span>
          ) : null
        }
      >
        {exceptions.map((check) => (
          <CheckRow key={checkKey(check)} check={check} />
        ))}
        {passed.length ? (
          <div className="px-5 py-3 text-sm">
            <Collapsible>
              <CollapsibleTrigger variant="disclosure">
                <CaretRightIcon aria-hidden="true" />
                {t("host.passedCount", passed.length)}
              </CollapsibleTrigger>
              <CollapsibleContent>
                <div className="-mx-5 mt-3 -mb-3 flex flex-col divide-y divide-v2-separator border-t border-v2-separator">
                  {passed.map((check) => (
                    <CheckRow key={checkKey(check)} check={check} />
                  ))}
                </div>
              </CollapsibleContent>
            </Collapsible>
          </div>
        ) : null}
      </Panel>

      {server.catalogError ? (
        <Alert variant="destructive">
          <AlertTitle>{t("host.catalog")}</AlertTitle>
          <AlertDescription>{server.catalogError}</AlertDescription>
        </Alert>
      ) : null}

      <p className="text-xs text-tertiary">
        {server.configExists
          ? t("host.configFrom", server.configPath)
          : t("host.configNew", server.configPath)}
      </p>
    </StepPage>
  )
}
