// The pieces of a plan dialog: its writes, the bindings that stop it, and a footer that applies
// the plan until its token runs out.
import type { UseMutationResult, UseQueryResult } from "@tanstack/react-query"
import type { ReactNode } from "react"
import WarningCircleIcon from "~icons/ph/warning-circle"

import type { Schemas } from "@/api/client"
import { DescribeYaml } from "@/components/describe-yaml"
import { Frame } from "@/components/members/frame"
import { LoadingRows } from "@/components/page"
import { COUNTDOWN_FROM_SECONDS, clock, useSecondsLeft } from "@/components/plan/expiry"
import { Problem } from "@/components/problem"
import { YamlBlock, YamlDiff } from "@/components/yaml"
import { useT } from "@/i18n"
import { kindName } from "@/i18n/kind-name"
import { subjectName } from "@/model/node-shell"
import { Alert, AlertDescription, AlertTitle } from "@zeabur/design-system/components/alert"
import { Button } from "@zeabur/design-system/components/button"

/** A plan dialog: planning, then what the plan does, and a footer that applies it. */
export function PlanFrame<P extends { expiresAt: string; token?: string | null }>({
  title,
  plan,
  apply,
  action,
  destructive,
  ready = (data) => data !== undefined,
  what,
  onClose,
  children,
}: {
  title: string
  plan: UseQueryResult<P>
  apply: UseMutationResult<unknown, Error, string>
  action: string
  destructive?: boolean
  /** The plan has something to apply. */
  ready?: (data: P | undefined) => boolean
  what?: string
  onClose: () => void
  children: (data: P) => ReactNode
}) {
  const t = useT()
  return (
    <Frame
      title={title}
      wide
      onClose={onClose}
      footer={
        <PlanFooter
          expiresAt={plan.data?.expiresAt}
          ready={!plan.isFetching && ready(plan.data)}
          token={plan.data?.token ?? undefined}
          action={action}
          destructive={destructive}
          applying={apply.isPending}
          onCancel={onClose}
          onReplan={() => void plan.refetch()}
          onApply={(token) => apply.mutate(token)}
        />
      }
    >
      {plan.isPending || plan.isFetching ? (
        <LoadingRows label={t("plan.planning")} rows={3} />
      ) : plan.isError ? (
        <Problem error={plan.error} what={what} />
      ) : (
        children(plan.data)
      )}
      {apply.isError ? <Problem error={apply.error} what={what} /> : null}
    </Frame>
  )
}

function PlanFooter({
  expiresAt,
  ready,
  token,
  action,
  destructive = false,
  applying,
  onCancel,
  onReplan,
  onApply,
}: {
  expiresAt: string | undefined
  /** Planned, and not planning again. */
  ready: boolean
  token: string | undefined
  action: string
  destructive?: boolean
  applying: boolean
  onCancel: () => void
  onReplan: () => void
  onApply: (token: string) => void
}) {
  const t = useT()
  const left = useSecondsLeft(expiresAt)
  const expired = left !== undefined && left <= 0
  return (
    <div className="flex w-full flex-wrap items-center gap-2">
      <Button variant="ghost" size="sm" onClick={onCancel}>
        {t("plan.cancel")}
      </Button>
      {ready && token ? <Expiry left={left} /> : null}
      <span className="ml-auto">
        {expired || (ready && !token) ? (
          <Button size="sm" variant="outline" onClick={onReplan}>
            {t("plan.replan")}
          </Button>
        ) : (
          <Button
            size="sm"
            variant={destructive ? "destructive" : "default"}
            disabled={!ready || !token}
            loading={applying}
            onClick={() => token && onApply(token)}
          >
            {action}
          </Button>
        )}
      </span>
    </div>
  )
}

function Expiry({ left }: { left: number | undefined }) {
  const t = useT()
  if (left === undefined || left >= COUNTDOWN_FROM_SECONDS) return null
  return (
    <span className="type-meta text-muted-foreground tabular-nums">
      {left <= 0 ? t("plan.expired") : t("plan.expiresIn", clock(left))}
    </span>
  )
}

export function PlanWrites({ writes }: { writes: Schemas["ShellWrite"][] }) {
  const t = useT()
  return (
    <DescribeYaml
      describe={
        <WrittenList
          items={writes.map((write) => ({
            kind: write.kind,
            name: write.name,
            state: t(`nodeShell.write.${write.action}`),
          }))}
        />
      }
      yaml={
        <div className="flex flex-col gap-4">
          {writes.map((write) => {
            const label = `${write.kind}/${write.name}`
            return (
              <WriteSection key={label} label={label} meta={t(`nodeShell.write.${write.action}`)}>
                {write.yaml ? <YamlBlock yaml={write.yaml} label={label} /> : null}
              </WriteSection>
            )
          })}
        </div>
      }
    />
  )
}

/** Each object a plan writes, as a diff from what is there now. */
export function PlanDiffs({
  writes,
}: {
  writes: {
    kind: string
    name: string
    namespace: string
    beforeYaml?: string | null
    afterYaml?: string | null
  }[]
}) {
  const t = useT()
  const stateOf = (write: (typeof writes)[number]) =>
    !write.beforeYaml
      ? t("plan.create")
      : !write.afterYaml
        ? t("plan.delete")
        : t("members.plan.changed")
  return (
    <DescribeYaml
      describe={
        <WrittenList
          items={writes.map((write) => ({
            kind: write.kind,
            name: write.name,
            state: stateOf(write),
            place: write.namespace,
          }))}
        />
      }
      yaml={
        <div className="flex flex-col gap-4">
          {writes.map((write) => (
            <WriteSection
              key={`${write.namespace}/${write.kind}/${write.name}`}
              label={`${write.kind}/${write.name}`}
              meta={write.namespace}
            >
              <YamlDiff before={write.beforeYaml ?? ""} after={write.afterYaml ?? ""} />
            </WriteSection>
          ))}
        </div>
      }
    />
  )
}

/** What a plan writes, by the plain name of each kind. */
export function WrittenList({
  items,
}: {
  items: { kind: string; name: string; state: string; place?: string }[]
}) {
  const t = useT()
  return (
    <ul className="flex flex-col gap-1.5 text-sm">
      {items.map((item) => (
        <li
          key={`${item.place ?? ""}/${item.kind}/${item.name}`}
          className="flex min-w-0 flex-wrap items-baseline gap-x-2"
        >
          <span>{kindName(t, item.kind)}</span>
          <span className="min-w-0 font-mono break-all" translate="no">
            {item.name}
          </span>
          <span className="type-meta text-muted-foreground">{item.state}</span>
          {item.place ? (
            <span className="type-meta text-muted-foreground" translate="no">
              {item.place}
            </span>
          ) : null}
        </li>
      ))}
    </ul>
  )
}

function WriteSection({
  label,
  meta,
  children,
}: {
  label: string
  meta: string
  children: ReactNode
}) {
  return (
    <section aria-label={label} className="flex min-w-0 flex-col gap-2">
      <span className="min-w-0 font-mono text-sm font-medium break-all" translate="no">
        {label}
        <span className="ml-2 font-sans font-normal type-meta text-muted-foreground">{meta}</span>
      </span>
      {children}
    </section>
  )
}

/** Someone besides cluster administrators could get into the pod: the plan stops here. */
export function Exposed({
  namespace,
  exposed,
}: {
  namespace: string
  exposed: Schemas["ShellExposure"][]
}) {
  const t = useT()
  return (
    <Alert variant="destructive">
      <WarningCircleIcon aria-hidden="true" />
      <AlertTitle>{t("nodeShell.exposedTitle")}</AlertTitle>
      <AlertDescription>
        <p>{t("nodeShell.exposed", namespace)}</p>
        <ul className="mt-2 flex flex-col gap-2">
          {exposed.map((binding) => (
            <li key={`${binding.kind}/${binding.name}`} className="flex flex-col gap-0.5">
              <span className="font-mono text-xs break-all" translate="no">
                {`${binding.kind}/${binding.name} → ${binding.role}`}
              </span>
              <span className="type-meta">
                <span translate="no">{binding.rights.join(", ")}</span>
                {t("common.sep")}
                <span translate="no">
                  {binding.subjects
                    .map((subject) => `${subject.kind} ${subjectName(subject)}`)
                    .join(", ")}
                </span>
              </span>
            </li>
          ))}
        </ul>
      </AlertDescription>
    </Alert>
  )
}
