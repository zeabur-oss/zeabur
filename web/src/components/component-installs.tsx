// The catalog component installs of this tab: the dialog of one component, the floating progress
// of the others, and a question before the tab closes while one runs.
import { useQuery } from "@tanstack/react-query"
import {
  createContext,
  lazy,
  Suspense,
  useContext,
  useEffect,
  useState,
  type ReactNode,
} from "react"

import { capabilitiesQuery } from "@/api/queries"
import { InstallIndicator } from "@/components/install-indicator"
import { installs } from "@/lib/installs"
import { buttonOf } from "@/model/component-button"

const ComponentPlan = lazy(() =>
  import("@/components/component-plan").then((m) => ({ default: m.ComponentPlan })),
)

const OpenComponent = createContext<((id: string) => void) | null>(null)

/** Opens the install or manage dialog of a catalog component, on the page the person is on. */
export function useOpenComponent() {
  const open = useContext(OpenComponent)
  if (!open) throw new Error("useOpenComponent is used outside ComponentInstalls")
  return open
}

/** Mounted once in the shell. Closing or reloading the tab asks first while an install runs,
 * since the install stops with the tab. */
export function ComponentInstalls({ children }: { children: ReactNode }) {
  const [opened, setOpened] = useState<string>()
  const running = installs.useValue().some((i) => i.running)
  useEffect(() => {
    if (!running) return
    const ask = (event: BeforeUnloadEvent) => event.preventDefault()
    window.addEventListener("beforeunload", ask)
    return () => window.removeEventListener("beforeunload", ask)
  }, [running])
  return (
    <OpenComponent value={setOpened}>
      {children}
      <InstallIndicator opened={opened} onOpen={setOpened} />
      {opened ? <OpenedPlan key={opened} id={opened} onClose={() => setOpened(undefined)} /> : null}
    </OpenComponent>
  )
}

function OpenedPlan({ id, onClose }: { id: string; onClose: () => void }) {
  const list = useQuery(capabilitiesQuery).data?.items
  if (!list) return null
  const component = list.flatMap((c) => c.components).find((x) => x.id === id)
  const installation = list.flatMap((c) => c.installations).find((i) => i.component === id)
  return (
    <Suspense fallback={null}>
      <ComponentPlan
        id={id}
        name={component?.displayName ?? id}
        installed={buttonOf(installation) === "manage"}
        onClose={onClose}
      />
    </Suspense>
  )
}
