import { installedEngines, NewDatabaseDrawer, useDatabases } from "@/components/databases"
import { ImageDrawer } from "@/components/add/image-drawer"
import { adding, type AddWhat } from "@/lib/adding"

/** The form "New" opened, wherever it was opened from. */
export function AddHost({ project, what }: { project: string; what: AddWhat }) {
  const close = () => adding.set(null)
  if (what === "image") return <ImageDrawer project={project} onClose={close} />
  return <DatabaseForm project={project} engine={what} onClose={close} />
}

function DatabaseForm({
  project,
  engine,
  onClose,
}: {
  project: string
  engine: "postgresql" | "mongodb"
  onClose: () => void
}) {
  const data = useDatabases(project).data
  if (!data) return null
  return (
    <NewDatabaseDrawer
      project={project}
      engines={installedEngines(data)}
      engine={engine}
      taken={data.items.map((d) => d.name)}
      onClose={onClose}
    />
  )
}
