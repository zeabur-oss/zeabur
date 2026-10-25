/** The person a Kubernetes user name stands for: an OIDC user name carries the issuer's prefix
 * (`zeabur:ada@example.com`), which is not theirs. `system:` names are Kubernetes' own. */
export function personOf(username: string): string {
  if (username.startsWith("system:")) return username
  const colon = username.lastIndexOf(":")
  return colon >= 0 ? username.slice(colon + 1) : username
}
