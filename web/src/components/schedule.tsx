import { useT } from "@/i18n"
import { backupTimeOf, timeWords } from "@/lib/backup-time"
import { describeCron } from "@/lib/cron"

/** A cron expression in words: one time a day or a week in the browser's time and in UTC; as
 * written when it has no words. */
export function Schedule({ cron }: { cron: string }) {
  const t = useT()
  const time = backupTimeOf(cron)
  const words = time ? timeWords(t, time) : describeCron(t, cron)
  if (!words) {
    return (
      <code className="type-mono" translate="no">
        {cron}
      </code>
    )
  }
  return <span>{words}</span>
}
