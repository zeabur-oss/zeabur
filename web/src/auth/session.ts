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
