// Resource quantities written for people in their locale: CPU in cores or millicores, memory and
// storage in MiB or GiB, shares as whole percents. The API sends every quantity the interface
// counts with as a number (millicores, bytes) next to the string as written, so nothing here
// parses one.
import { useI18n, useT } from "@/i18n"

/** Numbers as the pages write them. */
export function useQuantityFormat() {
  const { locale } = useI18n()
  const t = useT()
  const one = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 })
  const two = new Intl.NumberFormat(locale, { maximumFractionDigits: 2, minimumFractionDigits: 2 })
  const percent = new Intl.NumberFormat(locale, { style: "percent", maximumFractionDigits: 0 })
  /** GiB with one decimal from 1 GiB, whole MiB below. */
  const memoryParts = (bytes: number): [string, string] =>
    bytes >= 2 ** 30
      ? [one.format(bytes / 2 ** 30), "GiB"]
      : [one.format(Math.round(bytes / 2 ** 20)), "MiB"]
  return {
    /** Cores with at most one decimal, for capacities: `1.5`. */
    cores: (millicores: number) => one.format(millicores / 1000),
    /** Cores with two decimals, for measured usage and its axis: `0.25`. */
    coresExact: (millicores: number) => two.format(millicores / 1000),
    /** Measured usage in cores, with the unit: `0.25 cores`. */
    cpu: (millicores: number) => t("usage.cores", two.format(millicores / 1000)),
    millicores: (millicores: number) => `${Math.round(millicores)}m`,
    memoryParts,
    memory: (bytes: number) => memoryParts(bytes).join(" "),
    percent: (ratio: number) => percent.format(ratio),
  }
}
