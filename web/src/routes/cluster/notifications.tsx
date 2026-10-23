// Cluster → Notifications: where node, runtime-security and system alerts go.
import { Notifications } from "@/components/notifications"
import { useT } from "@/i18n"
import { ClusterFrame } from "@/routes/cluster/frame"

export function NotificationsPage() {
  const t = useT()
  return (
    <ClusterFrame tab="notifications">
      <p className="type-body text-secondary">{t("notify.clusterIntro")}</p>
      <Notifications />
    </ClusterFrame>
  )
}
