// Resource quantities written for people in their locale: CPU in cores or millicores, memory and
// storage in MiB or GiB, shares as whole percents. The API sends every quantity the interface
// counts with as a number (millicores, bytes) next to the string as written, so nothing here
// parses one.
import { useI18n, useT, type Translate } from "@/i18n"
import { exactBytes, parseAmount, type Dimension } from "@/lib/amount"

/** An amount written as Kubernetes does ("100m", "5Gi") in plain units: `0.1 cores`, `5 GiB`.
 * Text that is no amount stays as it is. */
export function plainAmount(
  t: Translate,
  raw: string,
  dimension: Dimension,
  locale?: string,
): string {
  const value = parseAmount(raw, dimension)
  if (value === undefined) return raw
  if (dimension === "cpu")
    return t(
      "usage.cores",
      new Intl.NumberFormat(locale, { maximumFractionDigits: 3 }).format(value),
    )
  const one = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 })
  return value >= 2 ** 30
    ? `${one.format(value / 2 ** 30)} GiB`
    : `${one.format(Math.round(value / 2 ** 20))} MiB`
}

/** Numbers as the pages write them. */
export function useQuantityFormat() {
  const { locale } = useI18n()
  const t = useT()
  const one = new Intl.NumberFormat(locale, { maximumFractionDigits: 1 })
  const two = new Intl.NumberFormat(locale, { maximumFractionDigits: 2, minimumFractionDigits: 2 })
  const fine = new Intl.NumberFormat(locale, { maximumFractionDigits: 3 })
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
    /** CPU in cores with up to three decimals and the unit: `0.1 cores`, `0.208 cores`. */
    millicores: (millicores: number) => t("usage.cores", fine.format(millicores / 1000)),
    /** Requested CPU, as `millicores`. */
    cpuRequest: (millicores: number) => t("usage.cores", fine.format(millicores / 1000)),
    memoryParts,
    memory: (bytes: number) => memoryParts(bytes).join(" "),
    percent: (ratio: number) => percent.format(ratio),
    /** An amount written as Kubernetes does ("100m", "5Gi"), in plain units; other text stays. */
    amount: (raw: string | null | undefined, dimension: Dimension) =>
      raw == null ? "" : plainAmount(t, raw, dimension, locale),
    /** The same for an input: exact, so that reading it back gives the amount again. */
    exact: (raw: string, dimension: Dimension) => {
      const value = parseAmount(raw, dimension)
      if (value === undefined) return raw
      return dimension === "cpu"
        ? t("usage.cores", fine.format(value))
        : exactBytes(value).join(" ")
    },
  }
}
