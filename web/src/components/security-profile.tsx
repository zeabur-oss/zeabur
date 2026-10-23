import { useQuery } from "@tanstack/react-query"
import { Select } from "@zeabur/design-system/components/select"

import { namespaceQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { permission, RefusedHint } from "@/components/refused"
import { useT } from "@/i18n"
import {
  lowersProject,
  profiles,
  RESTRICTED_FIELDS,
  type Level,
  type Profile,
} from "@/model/pod-security"
import { MANAGE_VERB } from "@/model/project"

/** The project's enforced Pod Security level and whether the person may change it. */
export function useProjectSecurity(project: string) {
  const namespace = useQuery(namespaceQuery(project)).data
  return { enforced: namespace?.podSecurity ?? undefined, canChange: namespace?.canAdopt }
}

/** Strict or Compatible, with what each means and, when Compatible lowers the project and the
 * person may not, why and the way to admin rights. */
export function SecurityProfileField({
  project,
  value,
  current,
  disabled,
  onChange,
}: {
  project: string
  value: Profile
  /** The level the service's pods meet now, when it exists. */
  current?: Level
  disabled?: boolean
  onChange: (profile: Profile) => void
}) {
  const t = useT()
  const { enforced, canChange } = useProjectSecurity(project)
  const lowers = lowersProject(value, enforced)
  const shown = current === "privileged" ? "privileged" : value
  return (
    <div className="flex flex-col gap-2">
      <FormField label={t("part.security")} hint={t(`security.about.${shown}`)}>
        <Select
          size="sm"
          value={shown}
          disabled={disabled || current === "privileged"}
          onValueChange={(profile) => onChange(profile as Profile)}
          items={[
            ...profiles.map((p) => ({ value: p, label: t(`security.profile.${p}`) })),
            ...(current === "privileged"
              ? [{ value: "privileged", label: t("security.profile.privileged") }]
              : []),
          ]}
        />
      </FormField>
      {lowers ? (
        <p className="type-meta text-muted-foreground">
          {canChange === "denied" ? t("security.needsAdmin") : t("security.lowers")}
        </p>
      ) : null}
      {lowers ? (
        <RefusedHint
          hint={canChange}
          what={t("security.what")}
          missing={[permission("patch", "namespaces"), permission(MANAGE_VERB, "namespaces")]}
        />
      ) : null}
      <KubeDetails
        facts={[
          ...(current ? [[t("security.kube.service"), current] as [string, string]] : []),
          [
            t("security.kube.project"),
            enforced ? `pod-security.kubernetes.io/enforce=${enforced}` : t("security.kube.unset"),
          ],
          [t("security.kube.strict"), RESTRICTED_FIELDS],
        ]}
      />
    </div>
  )
}
