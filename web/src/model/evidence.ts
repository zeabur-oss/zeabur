// An edge's evidence in words: "Its public domain", "Both read the secret db-credentials",
// "Named in the variable API_URL".
import type { Translate } from "@/i18n"
import type { Edge, Evidence } from "@/model/connections"

function evidenceText(evidence: Evidence, t: Translate): string {
  switch (evidence.basis) {
    case "route":
      return t("edge.route")
    case "sharedObject":
      return evidence.object.kind === "Secret"
        ? t("edge.sharedSecret", evidence.object.name)
        : evidence.object.kind === "ConfigMap"
          ? t("edge.sharedConfig", evidence.object.name)
          : t("edge.shared", evidence.object.kind, evidence.object.name)
    case "envValue":
      return t("edge.env", evidence.variable)
  }
}

export const edgeText = (edge: Edge, t: Translate) =>
  edge.evidence.map((e) => evidenceText(e, t)).join(t("common.sep"))
