import { useState } from "react"
import { toast } from "sonner"
import DotsThreeIcon from "~icons/ph/dots-three"
import KeyIcon from "~icons/ph/key"
import UserGearIcon from "~icons/ph/user-gear"

import type { Schemas } from "@/api/client"
import { Command } from "@/components/bits"
import { DocsLink } from "@/components/docs-link"
import { FormField } from "@/components/form-field"
import { GatedMenuItem, GateTip } from "@/components/gate"
import {
  AccessFields,
  ReviewFrame,
  useAccessLines,
  useRoleLine,
  type Chosen,
} from "@/components/members/access"
import { Frame } from "@/components/members/frame"
import { MemberPlan } from "@/components/members/member-plan"
import { YamlBlock } from "@/components/yaml"
import { useI18n, useT } from "@/i18n"
import { formatTime } from "@/lib/format"
import {
  DEFAULT_LIFETIME,
  expired,
  kubeconfig,
  LIFETIMES,
  startNamespace,
  validName,
} from "@/model/api-tokens"
import { chosenOf, grantsOf } from "@/model/members"
import type { Gate } from "@/model/rights"
import { Button } from "@zeabur-oss/design-system/components/button"
import { CopyInput } from "@zeabur-oss/design-system/components/copy-input"
import { Input } from "@zeabur-oss/design-system/components/input"
import { Menu, MenuContent, MenuTrigger } from "@zeabur-oss/design-system/components/menu"
import { Select } from "@zeabur-oss/design-system/components/select"

type Token = Schemas.ApiTokenView
type Issued = Schemas.IssuedTokenView

const DOCS = "reference/api-tokens"

type Open =
  | { step: "create" }
  | { step: "access"; token: Token }
  | { step: "renew"; token: Token }
  | { step: "delete"; token: Token }
  | { step: "shown"; issued: Issued; namespace?: string }

/** The API tokens: machine accounts for scripts and CI, with roles like a member's. */
export function ApiTokenList({
  tokens,
  projectRoles,
  manage,
}: {
  tokens: Token[]
  projectRoles: boolean
  manage: Gate
}) {
  const t = useT()
  const [open, setOpen] = useState<Open>()
  const close = () => setOpen(undefined)
  return (
    <section className="flex flex-col gap-3" aria-labelledby="api-tokens">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div className="flex min-w-0 flex-col gap-1">
          <h2 id="api-tokens" className="type-heading">
            {t("apiTokens.title")}
          </h2>
          <p className="type-meta text-muted-foreground">{t("apiTokens.about")}</p>
        </div>
        <GateTip gate={manage}>
          <Button size="sm" variant="outline" onClick={() => setOpen({ step: "create" })}>
            <KeyIcon aria-hidden="true" data-icon="inline-start" />
            {t("apiTokens.create")}
          </Button>
        </GateTip>
      </div>
      {tokens.length === 0 ? (
        <p className="text-sm text-muted-foreground">{t("apiTokens.none")}</p>
      ) : (
        <ul className="divide-y divide-main overflow-hidden rounded-xl border border-main bg-card text-sm">
          {tokens.map((token) => (
            <TokenRow
              key={token.name}
              token={token}
              manage={manage}
              onOpen={(step) => setOpen({ step, token })}
            />
          ))}
        </ul>
      )}
      {open ? (
        <Opened open={open} projectRoles={projectRoles} onOpen={setOpen} onClose={close} />
      ) : null}
    </section>
  )
}

function accessOf(token: Token): { role: Schemas.MemberRole; projects: Schemas.ProjectGrant[] } {
  return { role: "member", projects: token.projects }
}

function TokenRow({
  token,
  manage,
  onOpen,
}: {
  token: Token
  manage: Gate
  onOpen: (step: "access" | "renew" | "delete") => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const accessLines = useAccessLines()
  const access = token.admin ? [t("apiTokens.admin")] : accessLines(accessOf(token))
  const when = token.expiresAt
    ? t(
        expired(token.expiresAt, Date.now()) ? "apiTokens.expired" : "apiTokens.expires",
        formatTime(token.expiresAt, locale),
      )
    : ""
  const by = token.createdBy ? t("apiTokens.by", shortUser(token.createdBy)) : ""
  return (
    <li className="grid grid-cols-invitation-phone items-center gap-3 px-4 py-2.5 md:grid-cols-member md:gap-4">
      <span className="flex min-w-0 items-center gap-3">
        <span className="flex size-6 shrink-0 items-center justify-center rounded-full bg-muted text-muted-foreground">
          <KeyIcon aria-hidden="true" className="size-3.5" />
        </span>
        <span className="flex min-w-0 flex-col">
          <span className="truncate font-mono font-medium" translate="no" title={token.name}>
            {token.name}
          </span>
          <span className="truncate type-meta text-muted-foreground max-md:hidden">{by}</span>
          <span
            className="truncate type-meta text-muted-foreground md:hidden"
            title={t("common.list", access)}
          >
            {t("common.list", access)}
          </span>
          <span className="truncate type-meta text-muted-foreground md:hidden">{when}</span>
        </span>
      </span>
      <span className="flex min-w-0 flex-col text-muted-foreground max-md:hidden">
        {access.map((line) => (
          <span key={line} className="truncate" title={line}>
            {line}
          </span>
        ))}
      </span>
      <span className="text-muted-foreground max-md:hidden md:col-span-2">{when}</span>
      <span className="flex justify-end">
        <Menu>
          <MenuTrigger
            render={
              <Button variant="ghost" size="icon-sm" aria-label={t("apiTokens.more", token.name)} />
            }
          >
            <DotsThreeIcon aria-hidden="true" />
          </MenuTrigger>
          <MenuContent align="end">
            <GatedMenuItem gate={manage} onClick={() => onOpen("access")}>
              <UserGearIcon aria-hidden="true" />
              {t("roles.change")}
            </GatedMenuItem>
            <GatedMenuItem gate={manage} onClick={() => onOpen("renew")}>
              <KeyIcon aria-hidden="true" />
              {t("apiTokens.renew")}
            </GatedMenuItem>
            <GatedMenuItem gate={manage} onClick={() => onOpen("delete")}>
              {t("apiTokens.delete")}
            </GatedMenuItem>
          </MenuContent>
        </Menu>
      </span>
    </li>
  )
}

/** The dialog that is open: a form, its plan, or the token just issued. */
function Opened({
  open,
  projectRoles,
  onOpen,
  onClose,
}: {
  open: Open
  projectRoles: boolean
  onOpen: (open: Open) => void
  onClose: () => void
}) {
  const t = useT()
  switch (open.step) {
    case "create":
      return (
        <CreateDialog
          projectRoles={projectRoles}
          onClose={onClose}
          onIssued={(issued, namespace) => onOpen({ step: "shown", issued, namespace })}
        />
      )
    case "access":
      return <TokenAccess token={open.token} projectRoles={projectRoles} onClose={onClose} />
    case "renew":
      return (
        <RenewDialog
          name={open.token.name}
          onClose={onClose}
          onIssued={(issued) =>
            onOpen({ step: "shown", issued, namespace: startNamespace(open.token.projects) })
          }
        />
      )
    case "delete":
      return (
        <MemberPlan
          title={t("apiTokens.deleteTitle", open.token.name)}
          change={{ op: "deleteToken", name: open.token.name }}
          consequence={t("apiTokens.plan.revokes")}
          lines={[t("apiTokens.plan.delete", open.token.name)]}
          action={t("apiTokens.delete")}
          destructive
          onClose={onClose}
          onApplied={() => {
            toast.success(t("apiTokens.deleted", open.token.name))
            onClose()
          }}
        />
      )
    case "shown":
      return <TokenShown issued={open.issued} namespace={open.namespace} onClose={onClose} />
  }
}

function shortUser(user: string): string {
  return user.startsWith("zeabur:") ? user.slice("zeabur:".length) : user
}

/** What a token's access comes to, in the plan's words. */
function useAccessSentence(): (name: string, admin: boolean, chosen: Chosen) => string {
  const t = useT()
  const roleLine = useRoleLine()
  return (name, admin, chosen) =>
    admin
      ? t("apiTokens.plan.admin", name)
      : chosen.size === 0
        ? t("apiTokens.plan.noRole", name)
        : t("members.plan.gets", name, roleLine(grantsOf(chosen)))
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

interface Wanted {
  name: string
  admin: boolean
  chosen: Chosen
  seconds: number
}

function CreateDialog({
  projectRoles,
  onClose,
  onIssued,
}: {
  projectRoles: boolean
  onClose: () => void
  onIssued: (issued: Issued, namespace?: string) => void
}) {
  const t = useT()
  const [name, setName] = useState("")
  const [admin, setAdmin] = useState(false)
  const [chosen, setChosen] = useState<Chosen>(new Map())
  const [seconds, setSeconds] = useState(DEFAULT_LIFETIME)
  const [tried, setTried] = useState(false)
  const [reviewing, setReviewing] = useState(false)
  const badName = !validName(name)
  if (reviewing)
    return (
      <CreatePlan
        wanted={{ name, admin, chosen, seconds }}
        projectRoles={projectRoles}
        onBack={() => setReviewing(false)}
        onClose={onClose}
        onIssued={onIssued}
      />
    )
  return (
    <ReviewFrame
      title={t("apiTokens.createTitle")}
      onClose={onClose}
      onReview={() => {
        setTried(true)
        if (!badName) setReviewing(true)
      }}
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
      <AccessFields
        projectRoles={projectRoles}
        admin={admin}
        onAdmin={setAdmin}
        chosen={chosen}
        onChosen={setChosen}
        adminCopy={{ label: t("apiTokens.admin"), hint: t("apiTokens.adminHint") }}
      />
      <Lifetime value={seconds} onChange={setSeconds} />
    </ReviewFrame>
  )
}

function CreatePlan({
  wanted: { name, admin, chosen, seconds },
  projectRoles,
  onBack,
  onClose,
  onIssued,
}: {
  wanted: Wanted
  projectRoles: boolean
  onBack: () => void
  onClose: () => void
  onIssued: (issued: Issued, namespace?: string) => void
}) {
  const t = useT()
  const sentence = useAccessSentence()
  const projects = admin || !projectRoles ? [] : grantsOf(chosen)
  return (
    <MemberPlan
      title={t("apiTokens.createTitle")}
      change={{ op: "createToken", name, admin, projects, expirationSeconds: seconds }}
      consequence={admin ? t("apiTokens.plan.adminWarning") : undefined}
      lines={[
        sentence(name, admin, chosen),
        t("apiTokens.plan.lasts", t("apiTokens.lasts", seconds)),
      ]}
      action={t("apiTokens.create")}
      onBack={onBack}
      onClose={onClose}
      onApplied={(applied) =>
        applied.issued ? onIssued(applied.issued, startNamespace(projects)) : onClose()
      }
    />
  )
}

function TokenAccess({
  token,
  projectRoles,
  onClose,
}: {
  token: Token
  projectRoles: boolean
  onClose: () => void
}) {
  const t = useT()
  const sentence = useAccessSentence()
  const [admin, setAdmin] = useState(token.admin)
  const [chosen, setChosen] = useState<Chosen>(() => chosenOf(token))
  const [reviewing, setReviewing] = useState(false)
  if (reviewing)
    return (
      <MemberPlan
        title={t("roles.title", token.name)}
        change={{
          op: "tokenAccess",
          name: token.name,
          admin,
          projects: admin || !projectRoles ? [] : grantsOf(chosen),
        }}
        consequence={admin && !token.admin ? t("apiTokens.plan.adminWarning") : undefined}
        lines={[sentence(token.name, admin, chosen)]}
        action={t("roles.save")}
        onBack={() => setReviewing(false)}
        onClose={onClose}
        onApplied={() => {
          toast.success(t("roles.saved", token.name))
          onClose()
        }}
      />
    )
  return (
    <ReviewFrame
      title={t("roles.title", token.name)}
      onClose={onClose}
      onReview={() => setReviewing(true)}
    >
      <AccessFields
        projectRoles={projectRoles}
        admin={admin}
        onAdmin={setAdmin}
        chosen={chosen}
        onChosen={setChosen}
        adminCopy={{ label: t("apiTokens.admin"), hint: t("apiTokens.adminHint") }}
        held={token.projects.map((p) => p.project)}
      />
    </ReviewFrame>
  )
}

function RenewDialog({
  name,
  onClose,
  onIssued,
}: {
  name: string
  onClose: () => void
  onIssued: (issued: Issued) => void
}) {
  const t = useT()
  const [seconds, setSeconds] = useState(DEFAULT_LIFETIME)
  const [reviewing, setReviewing] = useState(false)
  if (reviewing)
    return (
      <MemberPlan
        title={t("apiTokens.renewTitle", name)}
        change={{ op: "renewToken", name, expirationSeconds: seconds }}
        lines={[
          t("apiTokens.plan.renew", name),
          t("apiTokens.plan.lasts", t("apiTokens.lasts", seconds)),
        ]}
        action={t("apiTokens.renew")}
        onBack={() => setReviewing(false)}
        onClose={onClose}
        onApplied={(applied) => (applied.issued ? onIssued(applied.issued) : onClose())}
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
            {t("create.review")}
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
function TokenShown({
  issued,
  namespace,
  onClose,
}: {
  issued: Issued
  namespace?: string
  onClose: () => void
}) {
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
      <YamlBlock yaml={kubeconfig(issued, namespace)} label={t("apiTokens.shown.kubeconfig")} />
      {issued.ca ? (
        <>
          <h3 className="mt-2 text-sm font-medium">{t("apiTokens.shown.ca")}</h3>
          <Command text={issued.ca} />
        </>
      ) : null}
    </Frame>
  )
}
