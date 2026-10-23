import { useQuery } from "@tanstack/react-query"
import { Link } from "@tanstack/react-router"
import { useState, type ReactElement } from "react"
import PlusIcon from "~icons/ph/plus"
import WarningFillIcon from "~icons/ph/warning-fill"

import { isStatus } from "@/api/client"
import { joinQuery, nodesQuery, queryClient } from "@/api/queries"
import { Ago, Segmented } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { GateTip, useClusterGate } from "@/components/gate"
import { PlanDrawer } from "@/components/plan/plan-drawer"
import { usePlanner, usePlanStep } from "@/components/plan/planner"
import { StatusDot } from "@/components/status"
import { YamlBlock } from "@/components/yaml"
import { useI18n, useT } from "@/i18n"
import { formatTime } from "@/lib/format"
import { toYaml } from "@/lib/yaml-out"
import {
  DEFAULT_LIFETIME,
  expiration,
  joined,
  joinScript,
  LIFETIMES,
  liveTokens,
  newToken,
  serverProblem,
  tokenSecret,
  tokenString,
  unavailable,
  type JoinToken,
  type Lifetime,
} from "@/model/join"
import { Alert, AlertDescription } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"
import { Input } from "@zeabur/design-system/components/input"
import { Tooltip } from "@zeabur/design-system/components/tooltip"

const WATCH_MS = 5_000
const SERVER_COMMAND = "zeabur-installer node add --role server"

function Closed({ reason, children }: { reason: string; children: ReactElement }) {
  return (
    <Tooltip content={reason}>
      <span className="inline-flex" tabIndex={0}>
        {children}
        <span className="sr-only">{reason}</span>
      </span>
    </Tooltip>
  )
}

/** The Nodes section's "Add node": closed with a reason on a cluster Zeabur cannot add to. */
export function AddNodeButton() {
  const t = useT()
  const gate = useClusterGate("changeNodes")
  const join = useQuery(joinQuery)
  const [open, setOpen] = useState(false)
  const why = join.data ? unavailable(join.data) : undefined
  const button = (
    <Button
      variant="outline"
      size="sm"
      onClick={() => setOpen(true)}
      disabled={!join.data || !!why}
    >
      <PlusIcon aria-hidden="true" data-icon="inline-start" />
      {t("join.add")}
    </Button>
  )
  return (
    <>
      {why && gate.open ? (
        <Closed reason={t(`join.unavailable.${why}`)}>{button}</Closed>
      ) : (
        <GateTip gate={gate}>{button}</GateTip>
      )}
      {open && join.data ? (
        <AddNodeDrawer
          server={join.data.server ?? ""}
          caHash={join.data.caHash ?? ""}
          expect={join.data.expect ?? undefined}
          taken={join.data.tokens.map((token) => token.id)}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </>
  )
}

type Stage =
  | { step: "form" }
  | { step: "applying"; token: JoinToken; until: string; before: Set<string> }
  | { step: "ready"; token: JoinToken; until: string; before: Set<string> }

function AddNodeDrawer({
  server: suggested,
  caHash,
  expect,
  taken,
  onClose,
}: {
  server: string
  caHash: string
  expect?: { provider: string; region: string }
  taken: string[]
  onClose: () => void
}) {
  const t = useT()
  const { locale } = useI18n()
  const [lifetime, setLifetime] = useState<Lifetime>(DEFAULT_LIFETIME)
  const [server, setServer] = useState(suggested)
  const [tried, setTried] = useState(false)
  const [stage, setStage] = useState<Stage>({ step: "form" })
  const [asked, setAsked] = useState<{ token: JoinToken; until: string } | undefined>()
  const problem = serverProblem(server)

  const step = usePlanStep(
    "kube-system",
    () => {
      if (!asked) return
      const nodes = queryClient.getQueryData(nodesQuery.queryKey)?.items ?? []
      setStage({ step: "applying", ...asked, before: new Set(nodes.map((n) => n.name)) })
    },
    () => {
      void queryClient.invalidateQueries({ queryKey: joinQuery.queryKey })
      setStage((s) => (s.step === "applying" ? { ...s, step: "ready" } : s))
    },
  )

  const review = () => {
    setTried(true)
    if (problem) return
    const token = newToken(taken)
    const until = expiration(new Date(), lifetime)
    setAsked({ token, until })
    step.plan({
      title: t("join.planTitle"),
      changes: [{ op: "create", yaml: toYaml([tokenSecret(token, until)]) }],
      notes: [t("join.planNote", formatTime(until, locale))],
      view: "/cluster",
    })
  }

  if (stage.step !== "form")
    return (
      <PlanDrawer
        step={step}
        onClose={onClose}
        title={t("join.title")}
        footer={
          <div className="flex w-full justify-end">
            <Button size="sm" onClick={onClose}>
              {t("join.done")}
            </Button>
          </div>
        }
      >
        {stage.step === "applying" ? (
          <p className="flex items-center gap-2 text-muted-foreground">
            <StatusDot health="progress" />
            {t("join.creating")}
          </p>
        ) : (
          <Ready
            script={joinScript({ server, token: tokenString(caHash, stage.token), expect })}
            until={formatTime(stage.until, locale)}
            before={stage.before}
            onClose={onClose}
          />
        )}
      </PlanDrawer>
    )

  return (
    <PlanDrawer
      step={step}
      onClose={onClose}
      title={t("join.title")}
      description={t("join.body")}
      footer={
        <div className="flex w-full justify-end gap-2">
          <Button variant="ghost" size="sm" onClick={onClose}>
            {t("edit.cancel")}
          </Button>
          <Button size="sm" onClick={review}>
            {t("edit.review")}
          </Button>
        </div>
      }
    >
      <form
        className="flex flex-col gap-5"
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <section aria-labelledby="join-before" className="flex flex-col gap-2">
          <h3 id="join-before" className="type-label">
            {t("join.before")}
          </h3>
          <Alert variant="warn" role="note">
            <WarningFillIcon aria-hidden="true" />
            <AlertDescription>{t("join.warn.sameRegion")}</AlertDescription>
          </Alert>
          <ul className="flex list-disc flex-col gap-1.5 pl-5 type-meta text-muted-foreground">
            <li>{t("join.warn.ports")}</li>
            <li>{t("join.warn.udp")}</li>
            <li>{t("join.warn.storageLocal")}</li>
          </ul>
        </section>
        <FormField label={t("join.role")} hint={t("join.workerHint")}>
          <p className="font-medium">{t("join.worker")}</p>
        </FormField>
        <FormField label={t("join.lifetime")} hint={t("join.lifetimeHint")}>
          <div>
            <Segmented
              label={t("join.lifetime")}
              value={String(lifetime)}
              onChange={(value) => setLifetime(Number(value) as Lifetime)}
              options={LIFETIMES.map((hours) => ({
                value: String(hours),
                label: t("join.hours", hours),
              }))}
            />
          </div>
        </FormField>
        <FormField
          label={t("join.server")}
          hint={t("join.serverHint")}
          errors={tried && problem ? [t(`join.serverProblem.${problem}`)] : []}
        >
          <Input
            size="sm"
            mono
            value={server}
            placeholder="https://10.0.0.5:9345"
            autoComplete="off"
            spellCheck={false}
            onChange={(event) => setServer(event.target.value)}
          />
        </FormField>
        <ServerNote />
      </form>
    </PlanDrawer>
  )
}

function ServerNote() {
  const t = useT()
  const [before, after] = t("join.serverNote", SERVER_COMMAND).split(SERVER_COMMAND)
  return (
    <div className="flex flex-col gap-1 border-t pt-4 type-meta text-muted-foreground">
      <p>
        {before}
        <code className="type-mono text-foreground" translate="no">
          {SERVER_COMMAND}
        </code>
        {after}
      </p>
      <p>{t("join.serversHa")}</p>
    </div>
  )
}

function Ready({
  script,
  until,
  before,
  onClose,
}: {
  script: string
  until: string
  before: Set<string>
  onClose: () => void
}) {
  const t = useT()
  const nodes = useQuery({ ...nodesQuery, refetchInterval: WATCH_MS })
  const node = nodes.data ? joined(before, nodes.data.items) : undefined
  return (
    <div className="flex flex-col gap-5">
      <p className="flex items-center gap-2">
        <StatusDot health="ok" />
        {t("join.created", until)}
      </p>
      <ol className="flex list-decimal flex-col gap-4 pl-5">
        <li>{t("join.stepBinary")}</li>
        <li className="flex flex-col gap-2">
          <span>{t("join.stepRun")}</span>
          <YamlBlock yaml={script} label={t("join.script")} />
          <span className="type-meta text-muted-foreground">{t("join.stepRunHint")}</span>
        </li>
      </ol>
      <Alert variant="warn" role="note">
        <WarningFillIcon aria-hidden="true" />
        <AlertDescription>{t("join.shownOnce")}</AlertDescription>
      </Alert>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-t pt-4" aria-live="polite">
        {nodes.isError && isStatus(nodes.error, 403) ? (
          <span className="text-muted-foreground">{t("join.cannotWatch")}</span>
        ) : node ? (
          <>
            <span className="flex items-center gap-2">
              <StatusDot health={node.ready ? "ok" : "progress"} />
              {node.ready ? t("join.joined", node.name) : t("join.joining", node.name)}
            </span>
            <Link
              to="/cluster"
              hash={`node-${node.name}`}
              onClick={onClose}
              className="type-meta underline-offset-2 hover:underline"
            >
              {t("join.viewNode")}
            </Link>
          </>
        ) : (
          <span className="flex items-center gap-2 text-muted-foreground">
            <StatusDot health="progress" />
            {t("join.waiting")}
          </span>
        )}
      </div>
    </div>
  )
}

/** Join tokens that still work, each with Revoke; nothing when there are none. */
export function JoinTokens() {
  const t = useT()
  const join = useQuery(joinQuery)
  const gate = useClusterGate("changeNodes")
  const planner = usePlanner(
    "kube-system",
    () => {
      void queryClient.invalidateQueries({ queryKey: joinQuery.queryKey })
    },
    true,
  )
  const tokens = join.data ? liveTokens(join.data.tokens, Date.now()) : []
  if (tokens.length === 0) return null
  const revoke = (id: string, name: string) =>
    planner.plan({
      title: t("join.revokeTitle", id),
      changes: [
        { op: "delete", resource: { group: "", version: "v1", resource: "secrets" }, name },
      ],
      notes: [t("join.revokeNote")],
      view: "/cluster",
    })
  return (
    <section aria-labelledby="join-tokens" className="flex flex-col gap-2">
      <h3 id="join-tokens" className="flex items-baseline gap-2.5 type-label">
        {t("join.tokens")}
        <span className="type-meta font-normal text-muted-foreground">{tokens.length}</span>
      </h3>
      <ul className="flex flex-col">
        {tokens.map((token) => (
          <li
            key={token.id}
            className="flex flex-wrap items-center gap-x-4 gap-y-1 border-t border-rule py-2"
          >
            <span className="type-mono" translate="no">
              {token.id}
            </span>
            <span className="type-meta text-muted-foreground">
              {token.managedBy === "zeabur"
                ? t("join.byZeabur")
                : token.managedBy === "zeabur-installer"
                  ? t("join.byInstaller")
                  : (token.description ?? "")}
            </span>
            <span className="flex items-center gap-1.5 type-meta text-muted-foreground sm:ml-auto">
              {token.expires ? (
                <>
                  {t("join.expires")}
                  <Ago iso={token.expires} />
                </>
              ) : (
                t("join.never")
              )}
            </span>
            <GateTip gate={gate}>
              <Button variant="outline" size="xs" onClick={() => revoke(token.id, token.secret)}>
                {t("join.revoke")}
              </Button>
            </GateTip>
          </li>
        ))}
      </ul>
      {planner.dialog}
    </section>
  )
}
