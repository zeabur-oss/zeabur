// An edge's evidence in words: "Ingress web", "Secret db-credentials",
// "API_URL".
import type { Translate } from "@/i18n"
import type { Edge, Evidence } from "@/model/connections"

function evidenceText(evidence: Evidence, t: Translate): string {
  switch (evidence.basis) {
    case "route":
      return t("edge.route", evidence.ingress)
    case "sharedObject":
      return t("edge.shared", evidence.object.kind, evidence.object.name)
    case "envValue":
      return t("edge.env", evidence.variable)
  }
}

export const edgeText = (edge: Edge, t: Translate) =>
  edge.evidence.map((e) => evidenceText(e, t)).join(t("common.sep"))
