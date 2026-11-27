import { expect, test } from "bun:test"

import type { Schemas } from "@/api/client"
import { objectWarnings } from "@/components/plan/kube-warnings"
import { translator } from "@/i18n"
import { en } from "@/i18n/en"

const object = (name: string, warnings?: string[]) =>
  ({ target: { kind: "Deployment", name }, warnings }) as unknown as Schemas.PlannedObject

test("each warning of a write names the object it is about", () => {
  expect(
    objectWarnings(translator(en), [
      object("web", ['would violate PodSecurity "restricted:latest"']),
      object("api"),
    ]),
  ).toStrictEqual([
    'Kubernetes warns about Deployment/web: would violate PodSecurity "restricted:latest"',
  ])
})
