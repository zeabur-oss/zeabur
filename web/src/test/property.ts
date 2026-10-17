// Property tests without a dependency: a property is checked on many generated inputs
// (Claessen and Hughes, "QuickCheck", ICFP 2000, https://doi.org/10.1145/351240.351266), here
// without shrinking. The inputs come from fixed seeds, so every run checks the same cases and a
// failure names the seed that reproduces it.

/** A source of generated values. */
export interface Random {
  /** An integer from `min` to `max`, both included. */
  int(min: number, max: number): number
  pick<T>(items: readonly T[]): T
  /** Up to `max` values made by `make`. */
  list<T>(max: number, make: () => T): T[]
}

/** Mulberry32, a small 32-bit generator; plenty for test inputs, never for secrets. */
function random(seed: number): Random {
  let state = seed >>> 0
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0
    let z = state
    z = Math.imul(z ^ (z >>> 15), z | 1)
    z ^= z + Math.imul(z ^ (z >>> 7), z | 61)
    return ((z ^ (z >>> 14)) >>> 0) / 2 ** 32
  }
  const int = (min: number, max: number) => min + Math.floor(next() * (max - min + 1))
  return {
    int,
    pick: (items) => items[int(0, items.length - 1)]!,
    list: (max, make) => Array.from({ length: int(0, max) }, make),
  }
}

/** Checks `property` on the inputs of `runs` seeds; a failure's message starts with its seed. */
export function forAll(property: (random: Random) => void, runs = 300): void {
  for (let seed = 1; seed <= runs; seed++) {
    try {
      property(random(seed))
    } catch (error) {
      if (error instanceof Error) error.message = `seed ${seed}: ${error.message}`
      throw error
    }
  }
}
