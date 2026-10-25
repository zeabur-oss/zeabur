// On this machine (`zeaburctl ui`) the API never answers `signed-out` and `signsIn` is false.
//
// The server refuses a change without `zeabur-request: 1`: another site cannot add a custom header
// to a cross-origin request without a CORS preflight (OWASP Cross-Site Request Forgery Prevention
// Cheat Sheet, "Employing Custom Request Headers for AJAX/API").

export const REQUEST_HEADERS = { "zeabur-request": "1" } as const

export function signInHref(location: Pick<Location, "pathname" | "search" | "hash">): string {
  const back = `${location.pathname}${location.search}${location.hash}`
  return back === "/" ? "/auth/login" : `/auth/login?return=${encodeURIComponent(back)}`
}

let leaving = false

/** Many requests fail at once; only one navigates. */
export function goToSignIn(): void {
  if (leaving) return
  leaving = true
  window.location.assign(signInHref(window.location))
}

export async function signOut(): Promise<void> {
  await fetch("/auth/logout", {
    method: "POST",
    headers: REQUEST_HEADERS,
    credentials: "same-origin",
  })
  leaving = true
  window.location.assign("/auth/login")
}

// A request with this header does not keep the session alive. Polls send it once the page has had
// no input for IDLE_AFTER_MS, so an open tab that nobody uses lets the session end.
export const BACKGROUND_HEADER = "x-zeabur-background"
export const IDLE_AFTER_MS = 60_000

const INPUT_EVENTS = [
  "pointerdown",
  "pointermove",
  "keydown",
  "touchstart",
  "wheel",
  "scroll",
  "focus",
]

let lastInput = Date.now()

/** Only a read made after a minute without input is background work. A change is always the
 * person's own. */
export function isBackground(method: string, lastInputAt: number, now: number): boolean {
  return method === "GET" && now - lastInputAt >= IDLE_AFTER_MS
}

export function backgroundNow(method: string): boolean {
  return isBackground(method, lastInput, Date.now())
}

export function followInput(target: Pick<EventTarget, "addEventListener"> = window): void {
  const mark = () => {
    lastInput = Date.now()
  }
  for (const type of INPUT_EVENTS)
    target.addEventListener(type, mark, { capture: true, passive: true })
}
