import { useQuery } from "@tanstack/react-query"
import { useState } from "react"

import type { Schemas } from "@/api/client"
import { certificateDatesQuery, clusterIssuersQuery } from "@/api/queries"
import { FormField } from "@/components/form-field"
import { KubeDetails } from "@/components/kube-details"
import { ReviewWord } from "@/components/plan/draft"
import { FormDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { TextFile } from "@/components/text-file"
import { useI18n, useT } from "@/i18n"
import { formatDate } from "@/lib/format"
import { canIssue, certificateNow, pemProblems, type CertificateNow } from "@/model/cluster-issuers"
import type { DomainDrawerProps } from "@/routes/service/domain-access"
import { Input } from "@zeabur/design-system/components/input"
import { RadioGroup, RadioOption } from "@zeabur/design-system/components/radio"
import { Select } from "@zeabur/design-system/components/select"

type Source = "issuer" | "own"
type Issuer = Schemas["IssuerView"]
type Choice = ReturnType<typeof useChoice>

const startIssuer = (now: CertificateNow, fallback?: string | null) =>
  now.as === "issuer" ? now.issuer : (fallback ?? "")

function useChoice(route: Schemas["Route"], host: string) {
  const now = certificateNow(route, host)
  const issuers = useQuery(clusterIssuersQuery).data
  const listed = issuers?.state === "installed" ? issuers.issuers : []
  const [source, setSource] = useState<Source>(now.as === "secret" ? "own" : "issuer")
  const [picked, setIssuer] = useState<string>()
  const [certificate, setCertificate] = useState("")
  const [key, setKey] = useState("")
  const issuer = picked ?? startIssuer(now, issuers?.default ?? listed[0]?.name)
  return {
    now,
    listed,
    chosen: listed.find((i) => i.name === issuer),
    source,
    setSource,
    issuer,
    setIssuer,
    certificate,
    setCertificate,
    key,
    setKey,
  }
}

/** Ingress-shim names one issuer for all of an Ingress's domains, so one of them can't leave. */
const issuingOthers = (route: Schemas["Route"], others: string[]) =>
  Boolean(route.issuer) && others.length > 0

const unchanged = (c: Choice) =>
  c.source === "issuer" && c.now.as === "issuer" && c.now.issuer === c.issuer

/** The certificate the plan asks for, once the form has one. */
function sourceOf(
  c: Choice,
  host: string,
  shared: boolean,
): Schemas["CertificateSource"] | undefined {
  if (c.source === "own") {
    const ok = !shared && pemProblems(c.certificate, c.key).length === 0
    return ok ? { from: "own", certificate: c.certificate, key: c.key } : undefined
  }
  const name = c.issuer.trim()
  const fits = c.chosen === undefined || canIssue(c.chosen, host)
  return name && fits && !unchanged(c) ? { from: "issuer", name } : undefined
}

/** Where one domain of an Ingress gets its certificate: an issuer, or the person's own. */
export default function DomainCertificateDrawer(props: DomainDrawerProps) {
  const { project, service, route, host, plan, onClose } = props
  const t = useT()
  const choice = useChoice(route, host)
  const [tried, setTried] = useState(false)
  const others = route.hostnames.filter((h) => h !== host)
  const review = () => {
    setTried(true)
    const certificate = sourceOf(choice, host, issuingOthers(route, others))
    if (!certificate) return
    plan({
      title: t("domainCert.planTitle", host),
      service: { kind: service.kind, name: service.name },
      changes: [
        {
          op: "certificate",
          route: { kind: "Ingress", name: route.name },
          hostname: host,
          certificate,
        },
      ],
    })
    onClose()
  }
  return (
    <FormDrawer
      onClose={onClose}
      title={t("domainCert.title", host)}
      description={t("domainCert.lead")}
      footer={
        <ReviewFooter
          onCancel={onClose}
          onReview={review}
          note={tried && unchanged(choice) ? t("domainCert.nothing") : undefined}
          label={<ReviewWord />}
        />
      }
    >
      <ReviewForm onReview={review}>
        <Now project={project} now={choice.now} />
        <fieldset className="flex flex-col gap-3">
          <legend className="mb-1.5 text-sm font-medium">{t("domainCert.from")}</legend>
          <RadioGroup
            value={choice.source}
            onValueChange={(value) => choice.setSource(value as Source)}
          >
            <RadioOption value="issuer" label={t("domainCert.fromIssuer")} />
            <RadioOption value="own" label={t("domainCert.fromOwn")} />
          </RadioGroup>
        </fieldset>
        {choice.source === "issuer" ? (
          <IssuerField choice={choice} host={host} others={others} />
        ) : (
          <OwnFields choice={choice} tried={tried} route={route} others={others} />
        )}
        <KubeDetails
          facts={[
            [t("domainCert.kube.issuer"), `Ingress/${route.name} cert-manager.io/cluster-issuer`],
            [t("domainCert.kube.own"), t("domainCert.kube.ownValue", route.name)],
            [t("domainCert.kube.tls"), `Ingress/${route.name} spec.tls`],
          ]}
        />
      </ReviewForm>
    </FormDrawer>
  )
}

function IssuerField({ choice, host, others }: { choice: Choice; host: string; others: string[] }) {
  const t = useT()
  const wrong: Issuer | undefined =
    choice.chosen && !canIssue(choice.chosen, host) ? choice.chosen : undefined
  return (
    <FormField
      label={t("domainCert.issuer")}
      hint={others.length > 0 ? t("domainCert.shared", t("common.list", others)) : undefined}
      errors={wrong ? [t("domainCert.needsDns", wrong.name)] : []}
    >
      {choice.listed.length > 0 ? (
        <Select
          size="sm"
          className="w-full"
          value={choice.issuer}
          onValueChange={(value) => choice.setIssuer(String(value))}
          items={choice.listed.map((i) => ({ value: i.name, label: i.name }))}
        />
      ) : (
        <Input
          size="sm"
          mono
          value={choice.issuer}
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => choice.setIssuer(event.target.value)}
        />
      )}
    </FormField>
  )
}

function OwnFields({
  choice,
  tried,
  route,
  others,
}: {
  choice: Choice
  tried: boolean
  route: Schemas["Route"]
  others: string[]
}) {
  const t = useT()
  const problems = tried ? pemProblems(choice.certificate, choice.key) : []
  if (issuingOthers(route, others)) {
    return (
      <p className="type-body text-warn-strong">
        {t("domainCert.ownShared", route.name, t("common.list", others))}
      </p>
    )
  }
  return (
    <>
      <TextFile
        label={t("domainCert.certificate")}
        hint={t("domainCert.certificateHint")}
        upload={t("domainCert.upload")}
        rows={5}
        accept=".pem,.crt,.cer"
        placeholder="-----BEGIN CERTIFICATE-----"
        value={choice.certificate}
        onChange={choice.setCertificate}
        errors={problems.includes("certificate") ? [t("domainCert.badCert")] : []}
      />
      <TextFile
        label={t("domainCert.key")}
        hint={t("domainCert.keyHint")}
        upload={t("domainCert.upload")}
        rows={5}
        accept=".pem,.key"
        placeholder="-----BEGIN PRIVATE KEY-----"
        value={choice.key}
        onChange={choice.setKey}
        errors={problems.includes("key") ? [t("domainCert.badKey")] : []}
      />
    </>
  )
}

function Now({ project, now }: { project: string; now: CertificateNow }) {
  const t = useT()
  const { locale } = useI18n()
  const secret = now.as === "none" ? undefined : now.secret
  const dates = useQuery({
    ...certificateDatesQuery(project, secret ?? ""),
    enabled: secret !== undefined,
  })
  const expires = dates.data ? t("domainCert.expires", formatDate(dates.data.notAfter, locale)) : ""
  const said = {
    issuer: () => t("domainCert.nowIssuer", now.as === "issuer" ? now.issuer : ""),
    secret: () => t("domainCert.nowOwn", secret ?? ""),
    none: () => t("domainCert.nowNone"),
  }[now.as]()
  return (
    <p className="type-body text-secondary" role="status">
      {expires ? `${said} ${expires}` : said}
    </p>
  )
}
