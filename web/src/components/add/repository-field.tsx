// "Deploy from Git": the repository, picked from what the cluster's GitHub App lets the project
// build, or typed as a Git URL.
import { useQuery } from "@tanstack/react-query"
import { useState } from "react"

import { projectGitHubAppQuery } from "@/api/github-app"
import { Segmented } from "@/components/bits"
import { FormField } from "@/components/form-field"
import { GitHubConnect } from "@/components/add/github-connect"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import { grantedRepository, repositoriesOf, type GitHubRepository } from "@/model/github-app"
import {
  Combobox,
  ComboboxContent,
  ComboboxEmpty,
  ComboboxInput,
  ComboboxItem,
  ComboboxList,
} from "@zeabur-oss/design-system/components/combobox"
import { Input } from "@zeabur-oss/design-system/components/input"

type Source = "github" | "url"

export interface GitHubSource {
  project: string
  /** The project's grants have been read, or could not be. */
  checked: boolean
  repositories: GitHubRepository[]
  /** The oldest list among the installations. */
  listedAt?: string
  source: Source
  setSource: (source: Source) => void
  /** The repository picked, while the source is GitHub. */
  picked?: GitHubRepository
  setPicked: (picked: GitHubRepository | null) => void
}

/** GitHub is the first choice when the project may build from an installation. */
export function useGitHubSource(project: string): GitHubSource {
  const grants = useQuery(projectGitHubAppQuery(project))
  const granted = grants.data
  const [chosen, setSource] = useState<Source>()
  const [picked, setPicked] = useState<GitHubRepository | null>(null)
  const repositories = repositoriesOf(granted)
  const source: Source = repositories.length > 0 ? (chosen ?? "github") : "url"
  return {
    project,
    checked: !grants.isPending,
    repositories,
    listedAt: granted?.installations
      .map((i) => i.refreshedAt)
      .sort()
      .at(0),
    source,
    setSource,
    picked: source === "github" ? (picked ?? undefined) : undefined,
    setPicked,
  }
}

/** The repository the App reads: the one picked, or a granted one whose URL was typed. */
export function appRepository(github: GitHubSource, typed: string): GitHubRepository | undefined {
  if (github.source === "github") return github.picked
  return grantedRepository(github.repositories, typed)
}

/** The picked repository's URL, or what was typed. */
export function repositoryUrl(github: GitHubSource, typed: string): string {
  if (github.source === "url") return typed
  return github.picked ? `https://github.com/${github.picked.fullName}` : ""
}

export function RepositoryField({
  github,
  url,
  onUrl,
  errors,
}: {
  github: GitHubSource
  url: string
  onUrl: (url: string) => void
  errors: string[]
}) {
  const t = useT()
  return (
    <>
      {github.checked && github.repositories.length === 0 ? (
        <GitHubConnect project={github.project} />
      ) : null}
      {github.repositories.length > 0 ? (
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-sm font-medium">{t("add.source")}</span>
          <Segmented
            label={t("add.source")}
            value={github.source}
            onChange={github.setSource}
            options={[
              { value: "github", label: t("add.source.github") },
              { value: "url", label: t("add.source.url") },
            ]}
          />
        </div>
      ) : null}
      <FormField label={t("add.field.repository")} errors={errors}>
        {github.source === "github" ? (
          <RepositoryPicker github={github} />
        ) : (
          <Input
            size="sm"
            mono
            value={url}
            placeholder="https://github.com/acme/web"
            autoComplete="off"
            spellCheck={false}
            autoFocus
            onChange={(event) => onUrl(event.target.value)}
          />
        )}
      </FormField>
    </>
  )
}

function RepositoryPicker({ github }: { github: GitHubSource }) {
  const t = useT()
  return (
    <Combobox
      items={github.repositories}
      value={github.picked ?? null}
      onValueChange={(next: GitHubRepository | null) => github.setPicked(next)}
      itemToStringLabel={(r: GitHubRepository) => r.fullName}
      isItemEqualToValue={(a: GitHubRepository, b: GitHubRepository) =>
        a.installationId === b.installationId && a.fullName === b.fullName
      }
    >
      <ComboboxInput size="sm" placeholder={t("add.githubSearch")} autoFocus spellCheck={false} />
      <ComboboxContent>
        <ComboboxEmpty>{t("add.githubEmpty")}</ComboboxEmpty>
        <ComboboxList>
          {(r: GitHubRepository) => (
            <ComboboxItem key={`${r.installationId}/${r.fullName}`} value={r}>
              <span translate="no">{r.fullName}</span>
            </ComboboxItem>
          )}
        </ComboboxList>
      </ComboboxContent>
    </Combobox>
  )
}

/** In place of the access choice: the App reads the repository. */
export function GitHubAccess({ listedAt }: { listedAt?: string }) {
  const t = useT()
  const { locale } = useI18n()
  return (
    <p className="type-meta text-muted-foreground">
      <span className="block">{t("add.githubAccess")}</span>
      {listedAt ? (
        <span className="block">{t("add.githubListed", formatDate(listedAt, locale))}</span>
      ) : null}
    </p>
  )
}
