// Signing in and out of the in-cluster server. On this machine (`engawactl ui`) nobody signs in:
// the API never answers `signed-out` there, and the session says `signsIn: false`.
//
// Every request the interface makes carries `engawa-request: 1`. The server refuses a change
// without it: another site cannot add a custom header to a cross-origin request without a CORS
// preflight, which engawa never grants (OWASP Cross-Site Request Forgery Prevention Cheat Sheet,
// "Employing Custom Request Headers for AJAX/API").

/** The header that marks a request as the interface's own. */
export const REQUEST_HEADERS = { "engawa-request": "1" } as const

/** Where to sign in, coming back to the page at `location` afterwards. */
export function signInHref(location: Pick<Location, "pathname" | "search" | "hash">): string {
  const back = `${location.pathname}${location.search}${location.hash}`
  return back === "/" ? "/auth/login" : `/auth/login?return=${encodeURIComponent(back)}`
}

let leaving = false

/** The session ended: sign in again, back to this page. Many requests fail at once; one leaves. */
export function goToSignIn(): void {
  if (leaving) return
  leaving = true
  window.location.assign(signInHref(window.location))
}

/** Ends the session; Genkan asks for the passkey before the next one. */
export async function signOut(): Promise<void> {
  await fetch("/auth/logout", {
    method: "POST",
    headers: REQUEST_HEADERS,
    credentials: "same-origin",
  })
  leaving = true
  window.location.assign("/auth/login")
}
