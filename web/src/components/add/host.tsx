import { installedEngines, NewDatabaseDrawer, useDatabases } from "@/components/databases"
import { ComposeDrawer } from "@/components/add/compose-drawer"
import { NewObjectStorageDrawer, useObjectStorage } from "@/components/object-storage"
import { GitDrawer } from "@/components/add/git-drawer"
import { ImageDrawer } from "@/components/add/image-drawer"
import { adding, type AddWhat } from "@/lib/adding"
import type { Engine } from "@/model/databases"

/** The form "New" opened, wherever it was opened from. */
export function AddHost({ project, what }: { project: string; what: AddWhat }) {
  const close = () => adding.set(null)
  if (what === "image") return <ImageDrawer project={project} onClose={close} />
  if (what === "git") return <GitDrawer project={project} onClose={close} />
  if (what === "compose") return <ComposeDrawer project={project} onClose={close} />
  if (what === "objectStorage") return <ObjectStorageForm project={project} onClose={close} />
  return <DatabaseForm project={project} engine={what} onClose={close} />
}

function DatabaseForm({
  project,
  engine,
  onClose,
}: {
  project: string
  engine: Engine
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

function ObjectStorageForm({ project, onClose }: { project: string; onClose: () => void }) {
  const data = useObjectStorage(project).data
  if (!data) return null
  return (
    <NewObjectStorageDrawer
      project={project}
      taken={data.items.map((s) => s.name)}
      onClose={onClose}
    />
  )
}
