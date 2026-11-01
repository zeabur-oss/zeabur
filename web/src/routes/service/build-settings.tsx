import { useState } from "react"

import { FormField } from "@/components/form-field"
import { GateNote, useGate } from "@/components/gate"
import { usePlanner } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { cn } from "@/lib/utils"
import type { Build } from "@/model/builds"
import {
  buildEnvSecret,
  buildOptionsChanges,
  buildOptionsOf,
  pathProblem,
} from "@/model/git-service"
import { parseEnv } from "@/model/image-service"
import { Button } from "@zeabur-oss/design-system/components/button"
import { Input } from "@zeabur-oss/design-system/components/input"
import { SectionHeader } from "@zeabur-oss/design-system/components/section-header"
import { Textarea } from "@zeabur-oss/design-system/components/textarea"

/** The directory, Dockerfile and build variables of each `Build` that makes the service's
 * images. */
export function BuildSection({ project, builds }: { project: string; builds: Build[] }) {
  const t = useT()
  const gate = useGate(project, "build")
  return (
    <section aria-labelledby="build-title" className="flex flex-col gap-3">
      <SectionHeader
        id="build-title"
        title={t("part.build")}
        description={t("buildOptions.hint")}
      />
      <GateNote gate={gate} />
      {builds.map((build) => (
        <BuildOptionsForm
          key={build.name}
          project={project}
          build={build}
          named={builds.length > 1}
          disabled={!gate.open}
        />
      ))}
    </section>
  )
}

function BuildOptionsForm({
  project,
  build,
  named,
  disabled,
}: {
  project: string
  build: Build
  named: boolean
  disabled: boolean
}) {
  const t = useT()
  const [options, setOptions] = useState(() => buildOptionsOf(build))
  const [added, setAdded] = useState("")
  const [tried, setTried] = useState(false)
  const planner = usePlanner(
    project,
    () => {
      setOptions((o) => ({ ...o, removed: [] }))
      setAdded("")
      setTried(false)
    },
    true,
  )
  const parsed = parseEnv(added)
  const set = parsed.variables.flatMap((v) =>
    "value" in v ? [{ name: v.name, value: v.value }] : [],
  )
  const problems = {
    directory: pathProblem(options.directory),
    dockerfile: pathProblem(options.dockerfile),
    env: parsed.bad.length > 0,
  }
  const changes = buildOptionsChanges(build, { ...options, set })
  const dockerfileShown = build.builder === "dockerfile" || !!build.dockerfile
  const toggle = (name: string) =>
    setOptions((o) => ({
      ...o,
      removed: o.removed.includes(name)
        ? o.removed.filter((n) => n !== name)
        : [...o.removed, name],
    }))
  const review = () => {
    setTried(true)
    if (Object.values(problems).some(Boolean) || changes.length === 0) return
    planner.plan({ title: t("title.buildOptions", build.name), changes })
  }
  const shown = (problem: unknown, text: string) => (tried && problem ? [text] : [])

  return (
    <form
      aria-label={named ? build.name : t("part.build")}
      className="flex flex-col gap-4 rounded-xl border border-v2-border bg-card p-5"
      onSubmit={(event) => {
        event.preventDefault()
        review()
      }}
    >
      {named ? (
        <h3 className="font-mono text-sm font-medium" translate="no">
          {build.name}
        </h3>
      ) : null}
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
        <FormField
          label={t("add.field.directory")}
          hint={t("add.directoryHint")}
          errors={shown(problems.directory, t("add.problem.directory"))}
        >
          <Input
            size="sm"
            mono
            value={options.directory}
            placeholder="apps/web"
            autoComplete="off"
            spellCheck={false}
            disabled={disabled}
            onChange={(event) => setOptions((o) => ({ ...o, directory: event.target.value }))}
          />
        </FormField>
        {dockerfileShown ? (
          <FormField
            label={t("add.field.dockerfile")}
            hint={t("add.dockerfileHint")}
            errors={shown(problems.dockerfile, t("add.problem.dockerfile"))}
          >
            <Input
              size="sm"
              mono
              value={options.dockerfile}
              placeholder="Dockerfile"
              autoComplete="off"
              spellCheck={false}
              disabled={disabled}
              onChange={(event) => setOptions((o) => ({ ...o, dockerfile: event.target.value }))}
            />
          </FormField>
        ) : null}
      </div>
      <div className="flex flex-col gap-2">
        <h4 className="text-sm font-medium">{t("add.buildVariables")}</h4>
        {build.env.length === 0 ? (
          <p className="type-meta text-muted-foreground">{t("buildOptions.none")}</p>
        ) : (
          <ul className="flex flex-col divide-y divide-v2-border rounded-lg border border-v2-border">
            {build.env.map((variable) => {
              const removed = options.removed.includes(variable.name)
              return (
                <li key={variable.name} className="flex min-w-0 items-center gap-3 px-3 py-2">
                  <div className="flex min-w-0 flex-1 flex-col">
                    <span
                      className={cn(
                        "truncate font-mono text-sm",
                        removed && "text-muted-foreground line-through",
                      )}
                      translate="no"
                    >
                      {variable.name}
                    </span>
                    <span className="truncate type-meta text-muted-foreground">
                      {variable.secret
                        ? t("buildOptions.fromSecret", variable.secret)
                        : t("buildOptions.literal")}
                    </span>
                  </div>
                  <Button
                    type="button"
                    variant="ghost"
                    size="xs"
                    disabled={disabled}
                    onClick={() => toggle(variable.name)}
                  >
                    {removed ? t("vars.undo") : t("buildOptions.remove")}
                  </Button>
                </li>
              )
            })}
          </ul>
        )}
        <FormField
          label={t("buildOptions.add")}
          hint={
            <>
              <span className="block">{t("buildOptions.addHint", buildEnvSecret(build.name))}</span>
              {build.builder === "dockerfile" ? (
                <span className="block">{t("add.buildVariablesDockerfile")}</span>
              ) : null}
            </>
          }
          errors={shown(problems.env, t("add.variablesBad", parsed.bad.join(", ")))}
        >
          <Textarea
            mono
            rows={3}
            value={added}
            placeholder="NPM_TOKEN=…"
            spellCheck={false}
            disabled={disabled}
            onChange={(event) => setAdded(event.target.value)}
          />
        </FormField>
      </div>
      <div className="flex justify-end">
        <Button type="submit" size="sm" disabled={disabled || changes.length === 0}>
          {t("edit.review")}
        </Button>
      </div>
      {planner.dialog}
    </form>
  )
}
