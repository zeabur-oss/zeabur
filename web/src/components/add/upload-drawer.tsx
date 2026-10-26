import { useNavigate } from "@tanstack/react-router"
import { useState } from "react"

import {
  MethodField,
  MoreSection,
  ServiceNameField,
  Note,
  BuiltPort,
  BuiltPublicUrl,
  useBuiltForm,
} from "@/components/add/built-service"
import { FolderDrop, useFolder } from "@/components/add/folder-drop"
import { PlanDrawer, ReviewFooter, ReviewForm } from "@/components/plan/plan-drawer"
import { usePlanStep } from "@/components/plan/planner"
import { useT } from "@/i18n"
import { serviceLink } from "@/lib/place"
import { nameFromImage } from "@/model/image-service"
import {
  createUploadService,
  newUploadService,
  uploadProblems,
  type UploadService,
} from "@/model/upload-service"
import { newRequest, startUpload } from "@/model/upload"

/** "Upload a folder": Zeabur Builder's Build, the service that runs what it builds, in one plan;
 * the folder goes up once the plan is applied (model/upload.ts). */
export function UploadDrawer({ project, onClose }: { project: string; onClose: () => void }) {
  const t = useT()
  const navigate = useNavigate()
  const folder = useFolder()
  const [request] = useState(newRequest)
  const ready = folder.state.phase === "ready" ? folder.state : undefined
  const built = useBuiltForm<UploadService>(project, newUploadService, (_, taken) =>
    ready?.name ? nameFromImage(ready.name.toLowerCase(), taken) : "",
  )
  const { name } = built
  const problems = uploadProblems(built.resolved, {
    taken: built.taken,
    badEnv: built.badEnv.length > 0,
    badBuildEnv: built.badBuildEnv.length > 0,
    hasFolder: !!ready,
  })
  const blocked = Object.values(problems).some(Boolean)

  const planner = usePlanStep(project, onClose, () => {
    if (ready) void startUpload({ project, build: name, blob: ready.packed.blob, request, t })
    void navigate(serviceLink(project, { kind: "Deployment", name }))
  })
  const review = () => {
    built.try()
    if (blocked) return
    planner.plan({
      title: t("add.planUpload", name),
      changes: [createUploadService(built.resolved, request)],
    })
  }

  return (
    <PlanDrawer
      step={planner}
      onClose={onClose}
      title={t("upload.title")}
      description={t("upload.body", project)}
      footer={<ReviewFooter onCancel={onClose} onReview={review} />}
    >
      <ReviewForm onReview={review}>
        <FolderDrop folder={folder} missing={built.tried && problems.folder && !ready} />
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
          <ServiceNameField built={built} problems={problems} />
          <BuiltPort built={built} problems={problems} />
        </div>
        <MethodField built={built} source="upload" />
        <BuiltPublicUrl built={built} problems={problems} />
        <MoreSection built={built} problems={problems} source="upload" />
        <Note>{t("upload.after")}</Note>
      </ReviewForm>
    </PlanDrawer>
  )
}
