import { KubeTip } from "@/components/kube-details"
import { useT } from "@/i18n"
import { describeCron } from "@/lib/cron"

/** A cron expression in words, with the expression on hover; as written when it has no words. */
export function Schedule({ cron }: { cron: string }) {
  const t = useT()
  const words = describeCron(t, cron)
  if (!words) {
    return (
      <code className="type-mono" translate="no">
        {cron}
      </code>
    )
  }
  return (
    <KubeTip facts={[[t("db.backup.schedule"), cron]]}>
      <span className="cursor-help">{words}</span>
    </KubeTip>
  )
}
