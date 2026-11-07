import { useQuery } from "@tanstack/react-query"

import { buildsQuery } from "@/api/queries"
import { GateTip, useGate } from "@/components/gate"
import { usePlanner, type PlanAsk } from "@/components/plan/planner"
import { useT, type Translate } from "@/i18n"
import type { Service } from "@/model/service"
import { portChange, wrongPort, type PortOffer, type WrongPort } from "@/model/wrong-port"
import { Button } from "@zeabur-oss/design-system/components/button"

/** What the card says in place of the failing traffic check. */
export function wrongPortWords(t: Translate, wrong: WrongPort) {
  return {
    cause: t("wrongPort.cause", wrong.port),
    why: t("wrongPort.why", wrong.port),
    fix: wrong.offer ? t("wrongPort.fixOffer", wrong.offer.to) : t("wrongPort.fix"),
  }
}

export function useWrongPort(project: string, service: Service): WrongPort | undefined {
  const builds = useQuery(buildsQuery(project)).data?.builds ?? []
  return wrongPort(service, builds, Date.now())
}

export const offerAsk = (t: Translate, service: Service, offer: PortOffer): PlanAsk => ({
  title: t("wrongPort.plan", service.name, offer.to),
  service: { kind: service.kind, name: service.name },
  changes: [portChange(service, offer)],
})

export function PortOfferButton({
  project,
  service,
  offer,
}: {
  project: string
  service: Service
  offer: PortOffer
}) {
  const t = useT()
  const planner = usePlanner(project)
  const gate = useGate(project, "changeServices")
  return (
    <>
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
    </>
  )
}
