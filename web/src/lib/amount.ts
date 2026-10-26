// Amounts of CPU and memory as people write them ("0.1 cores", "128 MiB") and as Kubernetes
// writes them ("100m", "128Mi"). Both parse; Kubernetes' form is what goes to the cluster.

export type Dimension = "cpu" | "bytes"

const factors: Record<string, number> = {
  "": 1,
  k: 1e3,
  K: 1e3,
  M: 1e6,
  G: 1e9,
  T: 1e12,
  P: 1e15,
  E: 1e18,
  Ki: 2 ** 10,
  Mi: 2 ** 20,
  Gi: 2 ** 30,
  Ti: 2 ** 40,
  Pi: 2 ** 50,
  Ei: 2 ** 60,
}

const bytesPattern = /^\+?(\d+(?:\.\d*)?|\.\d+)\s*(k|K|Ki|M|Mi|G|Gi|T|Ti|P|Pi|E|Ei)?(?:B)?$/
const cpuPattern = /^\+?(\d+(?:\.\d*)?|\.\d+)\s*(m|cores?|cpus?|核心|核)?$/i

/** Cores for a CPU, bytes for memory and storage; `undefined` for anything that is not an
 * amount. */
export function parseAmount(text: string, dimension: Dimension): number | undefined {
  const value = text.trim()
  if (dimension === "cpu") {
    const match = cpuPattern.exec(value)
    if (!match) return undefined
    const number = Number(match[1])
    return match[2] === "m" ? number / 1000 : number
  }
  const match = bytesPattern.exec(value)
  if (!match) return undefined
  return Number(match[1]) * (factors[match[2] ?? ""] ?? 1)
}

/** The Kubernetes form of an amount: `100m`, `2`, `128Mi`, `5Gi`. */
export function kubeAmount(amount: number, dimension: Dimension): string {
  if (dimension === "cpu") {
    const millis = Math.round(amount * 1000)
    return millis % 1000 === 0 ? String(millis / 1000) : `${millis}m`
  }
  for (const unit of ["Ti", "Gi", "Mi", "Ki"] as const) {
    const factor = factors[unit] ?? 1
    if (amount >= factor && amount % factor === 0) return `${amount / factor}${unit}`
  }
  return String(Math.round(amount))
}

/** What a person typed, as Kubernetes writes it; text that is not an amount stays as it is. */
export function toKube(text: string, dimension: Dimension): string {
  const amount = parseAmount(text, dimension)
  return amount === undefined ? text.trim() : kubeAmount(amount, dimension)
}

/** Whether Kubernetes accepts the text as a quantity (or it is empty). */
export function isAmount(text: string, dimension: Dimension): boolean {
  return text.trim() === "" || parseAmount(text, dimension) !== undefined
}

const binary = ["TiB", "GiB", "MiB", "KiB"] as const
const binaryFactor: Record<string, number> = {
  TiB: 2 ** 40,
  GiB: 2 ** 30,
  MiB: 2 ** 20,
  KiB: 2 ** 10,
}

/** Bytes in the largest binary unit that holds them without a fraction: `1500 MiB`, `5 GiB`. */
export function exactBytes(bytes: number): [string, string] {
  for (const unit of binary) {
    const factor = binaryFactor[unit] ?? 1
    if (bytes >= factor && bytes % factor === 0) return [String(bytes / factor), unit]
  }
  return [String(Math.round(bytes)), "B"]
}
