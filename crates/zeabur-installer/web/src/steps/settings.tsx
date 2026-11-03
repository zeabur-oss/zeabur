import { distributionTitle } from "@/api"
import type { Wizard } from "@/app"
import { Panel } from "@/components/bits"
import { useT } from "@/i18n"
import { StepPage } from "@/steps/frame"
import { Button, buttonVariants } from "@zeabur-oss/design-system/components/button"
import { Checkbox } from "@zeabur-oss/design-system/components/checkbox"

export function SettingsStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { config } = wizard
  const personal = wizard.use === "personal"
  const plan = wizard.planned && "plan" in wizard.planned ? wizard.planned.plan : null
  const reserved = plan && plan.use !== "personal" ? plan.memory.reservedMib : undefined
  return (
    <StepPage
      title={t("settings.title")}
      description={t("settings.description")}
      back={
        <Button variant="outline" onClick={() => wizard.go("components")}>
          {t("common.back")}
        </Button>
      }
      next={<Button onClick={() => wizard.go("review")}>{t("common.continue")}</Button>}
    >
      <Panel title={t("review.program")}>
        <div className="flex items-start gap-3">
          <Checkbox
            id="telemetry"
            className="mt-0.5"
            checked={wizard.active.telemetry !== false}
            onCheckedChange={wizard.setTelemetry}
          />
          <div className="flex min-w-0 flex-col items-start gap-1">
            <label htmlFor="telemetry" className="text-sm font-medium text-main">
              {t("review.program.label")}
            </label>
            <span className="text-sm text-secondary">{t("review.program.description")}</span>
            <a
              href="https://oss.zeabur.com/docs/zeabur/reference/telemetry/"
              target="_blank"
              rel="noreferrer"
              className={buttonVariants({ variant: "link", size: "sm" })}
            >
              {t("review.program.link")}
            </a>
          </div>
        </div>
      </Panel>

      {!personal && config.zeabur ? (
        <Panel title={t("review.updateCheck")}>
          <div className="flex items-start gap-3">
            <Checkbox
              id="update-check"
              className="mt-0.5"
              checked={config.updateCheck !== false}
              onCheckedChange={(updateCheck) => wizard.setConfig({ ...config, updateCheck })}
            />
            <div className="flex min-w-0 flex-col items-start gap-1">
              <label htmlFor="update-check" className="text-sm font-medium text-main">
                {t("review.updateCheck.label")}
              </label>
              <span className="text-sm text-secondary">{t("review.updateCheck.description")}</span>
              <a
                href="https://oss.zeabur.com/docs/zeabur/reference/telemetry/#update-check"
                target="_blank"
                rel="noreferrer"
                className={buttonVariants({ variant: "link", size: "sm" })}
              >
                {t("review.updateCheck.link")}
              </a>
            </div>
          </div>
        </Panel>
      ) : null}

      {plan && plan.use !== "personal" && reserved ? (
        <Panel title={t("settings.reserve")}>
          <div className="flex min-w-0 flex-col gap-1">
            <span className="text-sm font-medium text-main">
              {t("settings.reserve.value", reserved, distributionTitle[plan.cluster.distribution])}
            </span>
            <span className="text-sm text-secondary">{t("settings.reserve.hint")}</span>
          </div>
        </Panel>
      ) : null}
    </StepPage>
  )
}
