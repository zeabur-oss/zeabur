import { useQuery } from "@tanstack/react-query"
import type { ReactNode } from "react"
import InfoIcon from "~icons/ph/info"

import { buildsQuery } from "@/api/queries"

import { GateTip, useGate } from "@/components/gate"
import { usePlanner, type PlanAsk } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import type { Service } from "@/model/service"
import {
  portChange,
  portOffer,
  wrongPort,
  type PortOffer,
  type WrongPort,
} from "@/model/wrong-port"
import { Alert, AlertDescription } from "@zeabur-oss/design-system/components/alert"
import { Button } from "@zeabur-oss/design-system/components/button"

/** What the card says in place of the failing traffic check. */
export function wrongPortWords(t: Translate, wrong: WrongPort) {
  return {
    cause: t("wrongPort.cause", wrong.port),
    why: t("wrongPort.why", wrong.port),
    fix: t("wrongPort.fix"),
  }
}

/** The app not listening on the service's port, or else the image's port offered on its own. */
export function useWrongPort(project: string, service: Service) {
  const builds = useQuery(buildsQuery(project)).data?.builds ?? []
  const wrong = wrongPort(service, builds, Date.now())
  return { wrong, offer: wrong ? undefined : portOffer(service, builds) }
}

export const offerAsk = (t: Translate, service: Service, offer: PortOffer): PlanAsk => ({
  title: t("wrongPort.plan", service.name, offer.to),
  service: { kind: service.kind, name: service.name },
  changes: [portChange(service, offer)],
})

/** The image's port, said and offered with its button under the sentence. */
function PortOfferLine({
  project,
  service,
  offer,
  text,
}: {
  project: string
  service: Service
  offer: PortOffer
  text: string
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  return (
    <div className="flex flex-col items-start gap-2">
      <p className="text-foreground">{text}</p>
      <GateTip gate={gate}>
        <Button
          variant="outline"
          size="sm"
          onClick={() => void planner.plan(offerAsk(t, service, offer))}
        >
          {t("wrongPort.use", offer.to)}
        </Button>
      </GateTip>
      {planner.dialog}
    </div>
  )
}

/** What a failing check also takes down, after the image's port when it declares another. */
export function withOffer(
  t: Translate,
  project: string,
  service: Service,
  offer: PortOffer | undefined,
  also: ReactNode,
): ReactNode {
  if (!offer) return also
  return (
    <>
      <PortOfferLine
        project={project}
        service={service}
        offer={offer}
        text={t("wrongPort.declares", offer.to)}
      />
      {also}
    </>
  )
}

/** The image declares another port while nothing says the app is failing on its own. */
export function PortOfferCard({
  project,
  service,
  offer,
}: {
  project: string
  service: Service
  offer?: PortOffer
}) {
  const t = useT()
  if (!offer) return null
  return (
    <Alert variant="info">
      <InfoIcon aria-hidden="true" />
      <AlertDescription>
        <PortOfferLine
          project={project}
          service={service}
          offer={offer}
          text={t("wrongPort.offer", offer.to, offer.from)}
        />
      </AlertDescription>
    </Alert>
  )
}
