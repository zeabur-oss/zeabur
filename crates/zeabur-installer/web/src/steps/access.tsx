import { useEffect, useState } from "react"
import CheckCircleIcon from "~icons/ph/check-circle"
import GlobeIcon from "~icons/ph/globe-simple"
import FlaskIcon from "~icons/ph/flask"
import WarningIcon from "~icons/ph/warning"

import { api, type AccessMode } from "@/api"
import type { Wizard } from "@/app"
import { Panel, Segmented } from "@/components/bits"
import { useT } from "@/i18n"
import { StepPage } from "@/steps/frame"
import { Button } from "@zeabur/design-system/components/button"
import { Field, FieldDescription, FieldLabel } from "@zeabur/design-system/components/field"
import { Input } from "@zeabur/design-system/components/input"

const hostName =
  /^(?=.{1,253}$)([a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/

export function AccessStep({ wizard }: { wizard: Wizard }) {
  const t = useT()
  const { config, server } = wizard
  const access = config.access
  const address = server.addresses.find((a) => !a.includes(":"))
  const setAccess = (next: Partial<typeof access>) =>
    wizard.setConfig({ ...config, access: { ...access, ...next } })
  const same = access.zeabur !== "" && access.zeabur === access.identity
  const valid = hostName.test(access.zeabur) && hostName.test(access.identity) && !same
  return (
    <StepPage
      title={t("access.title")}
      description={t("access.description")}
      back={
        <Button variant="outline" onClick={() => wizard.go("host")}>
          {t("common.back")}
        </Button>
      }
      next={
        <Button disabled={!valid} onClick={() => wizard.go("components")}>
          {t("common.continue")}
        </Button>
      }
    >
      <Panel title={t("access.mode")}>
        <div className="flex flex-col gap-3">
          <Segmented<AccessMode>
            label={t("access.mode")}
            value={access.mode}
            onChange={(mode) => setAccess({ mode })}
            options={[
              {
                value: "domain",
                label: (
                  <>
                    <GlobeIcon aria-hidden="true" />
                    {t("access.domain")}
                  </>
                ),
              },
              {
                value: "local",
                label: (
                  <>
                    <FlaskIcon aria-hidden="true" />
                    {t("access.local")}
                  </>
                ),
              },
            ]}
          />
          <p className="max-w-prose text-sm text-secondary">
            {t(access.mode === "domain" ? "access.domainHint" : "access.localHint")}
          </p>
        </div>
      </Panel>

      <Panel
        title={t("access.names")}
        action={
          access.mode === "local" && address ? (
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                setAccess({
                  zeabur: `zeabur.${address}.sslip.io`,
                  identity: `login.${address}.sslip.io`,
                })
              }
            >
              {t("access.sslip")}
            </Button>
          ) : null
        }
      >
        <div className="flex flex-col gap-5">
          <HostField
            label={t("access.zeabur")}
            hint={t("access.zeaburHint")}
            placeholder="zeabur.example.com"
            value={access.zeabur}
            addresses={server.addresses}
            onChange={(zeabur) => setAccess({ zeabur })}
          />
          <HostField
            label={t("access.identity")}
            hint={t("access.identityHint")}
            placeholder="login.example.com"
            value={access.identity}
            addresses={server.addresses}
            onChange={(identity) => setAccess({ identity })}
            error={same ? t("access.same") : undefined}
          />
          {access.mode === "local" && address ? (
            <p className="text-xs text-tertiary">{t("access.sslipHint", address)}</p>
          ) : null}
        </div>
      </Panel>
    </StepPage>
  )
}

type Lookup = { name: string; addresses: string[] } | "checking"

function HostField({
  label,
  hint,
  placeholder,
  value,
  addresses,
  onChange,
  error,
}: {
  label: string
  hint: string
  placeholder: string
  value: string
  addresses: string[]
  onChange: (value: string) => void
  error?: string
}) {
  const t = useT()
  const [lookup, setLookup] = useState<Lookup | null>(null)
  const valid = hostName.test(value)
  useEffect(() => {
    if (!valid) return
    let current = true
    setLookup("checking")
    const timer = setTimeout(() => {
      api.resolve(value).then(
        (found) => current && setLookup(found),
        () => current && setLookup(null),
      )
    }, 400)
    return () => {
      current = false
      clearTimeout(timer)
    }
  }, [value, valid])

  const local =
    lookup && lookup !== "checking" ? lookup.addresses.filter((a) => addresses.includes(a)) : []
  const problem = error ?? (value && !valid ? t("access.invalid") : null)
  const status = problem ? (
    <Hint tone="bad">{problem}</Hint>
  ) : !valid || !lookup ? null : lookup === "checking" ? (
    <Hint>{t("access.checking")}</Hint>
  ) : lookup.addresses.length === 0 ? (
    <Hint tone="bad">{t("access.unresolved")}</Hint>
  ) : local.length ? (
    <Hint tone="good">{t("access.resolves", local.join(", "))}</Hint>
  ) : (
    <Hint tone="bad">{t("access.elsewhere", lookup.addresses.join(", "))}</Hint>
  )
  return (
    <Field className="max-w-md">
      <FieldLabel>{label}</FieldLabel>
      <FieldDescription>{hint}</FieldDescription>
      <Input
        value={value}
        placeholder={placeholder}
        mono
        autoComplete="off"
        aria-invalid={Boolean(error) || (value !== "" && !valid)}
        onChange={(event) => onChange(event.target.value.trim().toLowerCase())}
      />
      {status}
    </Field>
  )
}

function Hint({ tone, children }: { tone?: "good" | "bad"; children: string }) {
  return (
    <span
      className={
        tone === "good"
          ? "flex items-start gap-1.5 text-xs text-ok-strong"
          : tone === "bad"
            ? "flex items-start gap-1.5 text-xs text-warn-strong"
            : "flex items-start gap-1.5 text-xs text-tertiary"
      }
    >
      {tone === "good" ? (
        <CheckCircleIcon className="mt-px size-3.5 shrink-0" aria-hidden="true" />
      ) : tone === "bad" ? (
        <WarningIcon className="mt-px size-3.5 shrink-0" aria-hidden="true" />
      ) : null}
      {children}
    </span>
  )
}
