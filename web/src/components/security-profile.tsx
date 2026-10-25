import { useQuery } from "@tanstack/react-query"
import { Select } from "@zeabur/design-system/components/select"

import { namespaceQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { useT } from "@/i18n"
import { profiles, RESTRICTED_FIELDS, type Level, type Profile } from "@/model/pod-security"

/** Strict or Compatible, with what each means and, in a project that still enforces restricted,
 * that Compatible will not start there. */
export function SecurityProfileField({
  project,
  value,
  current,
  built,
  picked,
  disabled,
  onChange,
}: {
  project: string
  value: Profile
  /** The level the service's pods meet now, when it exists. */
  current?: Level
  /** Built from Git, where the build, not an image's name, decides the user. */
  built?: boolean
  /** Why Zeabur chose `value`, until the person chooses. */
  picked?: string
  disabled?: boolean
  onChange: (profile: Profile) => void
}) {
  const t = useT()
  const enforced = useQuery(namespaceQuery(project)).data?.podSecurity ?? undefined
  const shown = current === "privileged" ? "privileged" : value
  const about =
    built && shown === "restricted" ? t("security.aboutBuilt") : t(`security.about.${shown}`)
  return (
    <div className="flex flex-col gap-2">
      <FormField label={t("part.security")} hint={picked ? `${picked} ${about}` : about}>
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
      {shown === "baseline" && enforced === "restricted" ? (
        <p className="type-meta text-muted-foreground">{t("security.projectRestricted")}</p>
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
