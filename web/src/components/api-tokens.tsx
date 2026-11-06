import { useQuery } from "@tanstack/react-query"
import { useId, useState } from "react"
import DotsThreeIcon from "~icons/ph/dots-three"
import KeyIcon from "~icons/ph/key"

import type { Schemas } from "@/api/client"
import {
  apiTokensQuery,
  applyApiTokenChange,
  apiTokenPlanQuery,
  refreshApiTokens,
  type ApiTokenChange,
} from "@/api/api-tokens"
import { Command } from "@/components/bits"
import { DocsLink } from "@/components/docs-link"
import { FormField } from "@/components/form-field"
import { GateTip, useHintGate } from "@/components/gate"
import { Frame } from "@/components/members/frame"
import { WritesPlan } from "@/components/members/member-plan"
import { YamlBlock } from "@/components/yaml"
import { useI18n, useT } from "@/i18n"
import { formatTime } from "@/lib/format"
import type { Gate } from "@/model/rights"
import {
  clusterRefusal,
  DEFAULT_LIFETIME,
  expired,
  kubeconfig,
  LIFETIMES,
  roleFor,
  validName,
} from "@/model/api-tokens"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Card, CardRow, SectionLabel } from "@zeabur-oss/design-system/components/card"
import { Checkbox } from "@zeabur-oss/design-system/components/checkbox"
import { CopyInput } from "@zeabur-oss/design-system/components/copy-input"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Label } from "@zeabur-oss/design-system/components/label"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@zeabur-oss/design-system/components/menu"
import { RadioGroup, RadioOption } from "@zeabur-oss/design-system/components/radio"
import { Select } from "@zeabur-oss/design-system/components/select"

type Role = Schemas.ProjectRole
type Token = Schemas.ApiTokenView
type Issued = Schemas.IssuedTokenView

const DOCS = "reference/api-tokens"

type Open =
  | { step: "create" }
  | { step: "renew"; name: string }
  | { step: "delete"; name: string }
  | { step: "shown"; issued: Issued }

/** A project's API tokens, for scripts and CI that call kube-apiserver with a role in it. Hidden
 * for a namespace that is not a project and when its ServiceAccounts cannot be read. */
export function ApiTokens({ namespace }: { namespace: { name: string; adopted: boolean } }) {
  const project = namespace.name
  const t = useT()
  const tokens = useQuery({ ...apiTokensQuery(project), enabled: namespace.adopted })
  const [open, setOpen] = useState<Open>()
  const data = tokens.data
  const create = useHintGate("changeApiTokens", data ? creatable(data) : undefined)
  const renew = useHintGate("changeApiTokens", data?.canRenew)
  const remove = useHintGate("deleteApiTokens", data?.canDelete)
  if (!namespace.adopted || !data) return null
  return (
    <section data-testid="api-tokens">
      <SectionLabel>{t("apiTokens.title")}</SectionLabel>
      <p className="mb-3 type-meta text-muted-foreground">{t("apiTokens.about")}</p>
      <Card variant="list">
        {data.items.map((token) => (
          <TokenRow
            key={token.name}
            token={token}
            renew={renew}
            remove={remove}
            onOpen={(step) => setOpen({ step, name: token.name })}
          />
        ))}
        <CardRow
          label={data.items.length === 0 ? t("apiTokens.none") : t("apiTokens.another")}
          description={t("apiTokens.createHint")}
        >
          <GateTip gate={create}>
            <Button variant="outline" size="sm" onClick={() => setOpen({ step: "create" })}>
              <KeyIcon aria-hidden="true" data-icon="inline-start" />
              {t("apiTokens.create")}
            </Button>
          </GateTip>
        </CardRow>
      </Card>
      {open ? (
        <Opened
          project={project}
          open={open}
          choices={data.choices}
          onOpen={setOpen}
          onClose={() => setOpen(undefined)}
        />
      ) : null}
    </section>
  )
}

function creatable(data: Schemas.ApiTokens): Schemas.Permission {
  return data.choices.some((c) => c.permission !== "denied") ? "allowed" : "denied"
}

function TokenRow({
  token,
  renew,
  remove,
  onOpen,
}: {
  token: Token
  renew: Gate
  remove: Gate
  onOpen: (step: "renew" | "delete") => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const when = token.expiresAt
    ? t(
        expired(token.expiresAt, Date.now()) ? "apiTokens.expired" : "apiTokens.expires",
        formatTime(token.expiresAt, locale),
      )
    : ""
  const by = token.createdBy ? t("apiTokens.by", shortUser(token.createdBy)) : ""
  return (
    <CardRow
      label={
        <span className="truncate type-mono" translate="no">
          {token.name}
        </span>
      }
      description={
        <span className="flex flex-col gap-0.5">
          <span>{roleText(t, token)}</span>
          <span className="type-meta text-muted-foreground">
            {[by, when].filter(Boolean).join(t("common.sep"))}
          </span>
        </span>
      }
    >
      <span className="flex items-center gap-1">
        <GateTip gate={renew}>
          <Button size="sm" variant="outline" onClick={() => onOpen("renew")}>
            {t("apiTokens.renew")}
          </Button>
        </GateTip>
        <Menu>
          <GateTip gate={remove}>
            <MenuTrigger
              render={
                <Button
                  variant="ghost"
                  size="icon-sm"
                  aria-label={t("apiTokens.more", token.name)}
                />
              }
            >
              <DotsThreeIcon aria-hidden="true" />
            </MenuTrigger>
          </GateTip>
          <MenuContent align="end">
            <MenuItem onClick={() => onOpen("delete")}>{t("apiTokens.delete")}</MenuItem>
          </MenuContent>
        </Menu>
      </span>
    </CardRow>
  )
}

/** The dialog that is open: a form, its plan, or the token just issued. */
function Opened({
  project,
  open,
  choices,
  onOpen,
  onClose,
}: {
  project: string
  open: Open
  choices: readonly Schemas.RoleChoice[]
  onOpen: (open: Open) => void
  onClose: () => void
}) {
  const t = useT()
  const shown = (issued: Issued) => onOpen({ step: "shown", issued })
  switch (open.step) {
    case "create":
      return <CreateDialog project={project} choices={choices} onClose={onClose} onIssued={shown} />
    case "renew":
      return <RenewDialog project={project} name={open.name} onClose={onClose} onIssued={shown} />
    case "delete":
      return (
        <TokenPlan
          project={project}
          change={{ op: "delete", name: open.name }}
          title={t("apiTokens.deleteTitle", open.name)}
          consequence={t("apiTokens.plan.revokes")}
          lines={[t("apiTokens.plan.delete", open.name)]}
          action={t("apiTokens.delete")}
          destructive
          onClose={onClose}
          onIssued={onClose}
        />
      )
    case "shown":
      return <TokenShown issued={open.issued} onClose={onClose} />
  }
}

function shortUser(user: string): string {
  return user.startsWith("zeabur:") ? user.slice("zeabur:".length) : user
}

function roleText(t: ReturnType<typeof useT>, token: Token): string {
  const role = token.role ? t(`roles.${token.role}`) : t("apiTokens.otherRole")
  return token.cluster ? t("apiTokens.everywhere", role) : role
}

function TokenPlan({
  project,
  change,
  onIssued,
  ...copy
}: {
  project: string
  change: ApiTokenChange
  title: string
  consequence?: string
  lines: string[]
  action: string
  destructive?: boolean
  onBack?: () => void
  onClose: () => void
  onIssued: (issued: Issued) => void
}) {
  return (
    <WritesPlan
      {...copy}
      planQuery={apiTokenPlanQuery(project, change)}
      apply={(token) => applyApiTokenChange(project, token)}
      refresh={() => refreshApiTokens(project)}
      onApplied={(applied) => (applied.issued ? onIssued(applied.issued) : copy.onClose())}
    />
  )
}

function Lifetime({ value, onChange }: { value: number; onChange: (seconds: number) => void }) {
  const t = useT()
  return (
    <FormField label={t("apiTokens.lifetime")} hint={t("apiTokens.lifetimeHint")}>
      <Select
        size="sm"
        className="w-full"
        items={LIFETIMES.map((seconds) => ({
          value: String(seconds),
          label: t("apiTokens.lasts", seconds),
        }))}
        value={String(value)}
        onValueChange={(next) => onChange(Number(next))}
      />
    </FormField>
  )
}

function CreateDialog({
  project,
  choices,
  onClose,
  onIssued,
}: {
  project: string
  choices: readonly Schemas.RoleChoice[]
  onClose: () => void
  onIssued: (issued: Issued) => void
}) {
  const t = useT()
  const [name, setName] = useState("")
  const [cluster, setCluster] = useState(false)
  const [picked, setPicked] = useState<Role>()
  const [seconds, setSeconds] = useState(DEFAULT_LIFETIME)
  const [tried, setTried] = useState(false)
  const [reviewing, setReviewing] = useState(false)
  const role = roleFor(choices, cluster, picked)
  const badName = !validName(name)
  if (reviewing && role) {
    const roleName = t(`roles.${role}`)
    return (
      <TokenPlan
        project={project}
        change={{ op: "create", name, role, cluster, expirationSeconds: seconds }}
        title={t("apiTokens.createTitle")}
        lines={[
          cluster
            ? t("apiTokens.plan.createEverywhere", name, roleName)
            : t("apiTokens.plan.create", name, roleName),
          t("apiTokens.plan.lasts", t("apiTokens.lasts", seconds)),
        ]}
        action={t("apiTokens.create")}
        onBack={() => setReviewing(false)}
        onClose={onClose}
        onIssued={onIssued}
      />
    )
  }
  return (
    <Frame
      title={t("apiTokens.createTitle")}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button
            size="sm"
            disabled={!role}
            onClick={() => {
              setTried(true)
              if (!badName) setReviewing(true)
            }}
          >
            {t("apiTokens.review")}
          </Button>
        </>
      }
    >
      <FormField
        label={t("apiTokens.name")}
        hint={t("apiTokens.nameHint")}
        errors={tried && badName ? [t("apiTokens.nameProblem")] : []}
      >
        <Input
          size="sm"
          value={name}
          autoComplete="off"
          spellCheck={false}
          placeholder="github-actions"
          onChange={(event) => setName(event.target.value.trim())}
        />
      </FormField>
      <RolePicker
        choices={choices.filter((c) => c.cluster === cluster)}
        role={role}
        onPick={setPicked}
      />
      <Everywhere choices={choices} checked={cluster} onChange={setCluster} />
      <Lifetime value={seconds} onChange={setSeconds} />
    </Frame>
  )
}

function RolePicker({
  choices,
  role,
  onPick,
}: {
  choices: readonly Schemas.RoleChoice[]
  role: Role | undefined
  onPick: (role: Role) => void
}) {
  const t = useT()
  return (
    <fieldset className="flex min-w-0 flex-col gap-2">
      <legend className="mb-1.5 text-sm font-medium">{t("apiTokens.role")}</legend>
      <RadioGroup value={role ?? ""} onValueChange={(next) => onPick(next as Role)}>
        {choices.map((choice) => (
          <RadioOption
            key={choice.role}
            value={choice.role}
            disabled={choice.permission === "denied"}
            label={t(`roles.${choice.role}`)}
            description={
              choice.refusal
                ? t(`apiTokens.refusal.${choice.refusal}`)
                : t(`apiTokens.roleHint.${choice.role}`)
            }
          />
        ))}
      </RadioGroup>
    </fieldset>
  )
}

/** Whether the role applies in every namespace: for admins, and greyed out with why for others. */
function Everywhere({
  choices,
  checked,
  onChange,
}: {
  choices: readonly Schemas.RoleChoice[]
  checked: boolean
  onChange: (checked: boolean) => void
}) {
  const t = useT()
  const id = useId()
  const refusal = clusterRefusal(choices)
  return (
    <div className="flex items-start gap-2.5">
      <Checkbox
        id={id}
        checked={checked}
        disabled={refusal !== undefined}
        onCheckedChange={(next) => onChange(next)}
        className="mt-0.5"
      />
      <div className="flex flex-col gap-0.5">
        <Label htmlFor={id}>{t("apiTokens.cluster")}</Label>
        <span className="type-meta text-muted-foreground">
          {refusal ? t(`apiTokens.refusal.${refusal}`) : t("apiTokens.clusterHint")}
        </span>
      </div>
    </div>
  )
}

function RenewDialog({
  project,
  name,
  onClose,
  onIssued,
}: {
  project: string
  name: string
  onClose: () => void
  onIssued: (issued: Issued) => void
}) {
  const t = useT()
  const [seconds, setSeconds] = useState(DEFAULT_LIFETIME)
  const [reviewing, setReviewing] = useState(false)
  if (reviewing)
    return (
      <TokenPlan
        project={project}
        change={{ op: "renew", name, expirationSeconds: seconds }}
        title={t("apiTokens.renewTitle", name)}
        lines={[
          t("apiTokens.plan.renew", name),
          t("apiTokens.plan.lasts", t("apiTokens.lasts", seconds)),
        ]}
        action={t("apiTokens.renew")}
        onBack={() => setReviewing(false)}
        onClose={onClose}
        onIssued={onIssued}
      />
    )
  return (
    <Frame
      title={t("apiTokens.renewTitle", name)}
      onClose={onClose}
      footer={
        <>
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("plan.cancel")}
          </Button>
          <Button size="sm" onClick={() => setReviewing(true)}>
            {t("apiTokens.review")}
          </Button>
        </>
      }
    >
      <p>{t("apiTokens.renewAbout")}</p>
      <Lifetime value={seconds} onChange={setSeconds} />
    </Frame>
  )
}

/** The token, once: copy it now, with the CA and a kubeconfig that only needs the address. */
function TokenShown({ issued, onClose }: { issued: Issued; onClose: () => void }) {
  const t = useT()
  const { locale } = useI18n()
  return (
    <Frame
      title={t("apiTokens.shownTitle", issued.name)}
      wide
      onClose={onClose}
      footer={
        <Button size="sm" onClick={onClose}>
          {t("link.done")}
        </Button>
      }
    >
      <p>{t("apiTokens.shown.lead")}</p>
      <CopyInput value={issued.token} copyLabel={t("apiTokens.shown.copy")} />
      <p className="text-muted-foreground">
        {t("common.sentences", [
          t("apiTokens.expires", formatTime(issued.expiresAt, locale)),
          issued.recorded ? undefined : t("apiTokens.shown.notRecorded"),
        ])}
      </p>
      <DocsLink page={DOCS}>{t("apiTokens.docs")}</DocsLink>
      <h3 className="mt-2 text-sm font-medium">{t("apiTokens.shown.kubeconfig")}</h3>
      <p className="text-muted-foreground">{t("apiTokens.shown.kubeconfigHint")}</p>
      <YamlBlock yaml={kubeconfig(issued)} label={t("apiTokens.shown.kubeconfig")} />
      {issued.ca ? (
        <>
          <h3 className="mt-2 text-sm font-medium">{t("apiTokens.shown.ca")}</h3>
          <Command text={issued.ca} />
        </>
      ) : null}
    </Frame>
  )
}
