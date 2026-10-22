# zeabur-auth

How the in-cluster server signs people in through Zeabur Identity, keeps their sessions, and tells its
own page's requests from other sites'.

## Hides

- **The OAuth client** ([`client`]): Zeabur is a confidential client of Zeabur Identity (authorization
  code with PKCE S256, a client secret, `state`, an exact redirect URI), configured from
  `<issuer>/.well-known/openid-configuration`. It is written on the HTTP client stack kube-rs
  already ships (hyper, rustls with aws-lc-rs); there is no OpenID Connect or JOSE library, and
  no ID token is verified: who the person is comes from kube-apiserver (`SelfSubjectReview` with
  the access token), which validated the token itself.
- **Where people manage how they sign in** ([`passkey`]): the `manage` URL of the issuer's
  `/.well-known/passkey-endpoints` (W3C Passkey Endpoints), read with a short deadline and kept
  for a while. Zeabur holds no credentials of its own.
- **The sign-in in progress** ([`flow`]): `state`, the PKCE verifier and where to go afterwards,
  sealed into a short-lived cookie, so nothing is stored for a visitor who has not signed in.
- **Sessions** ([`session`]): an opaque random identifier in a cookie; the tokens stay in this
  process's memory. Renewal shortly before expiry, one at a time per session; idle and absolute
  limits; administrator rights held for a few minutes.

## Place in the workspace

Level 2: depends on `zeabur-core` only. Only the `zeabur` binary uses it; `zeaburctl ui` signs
nobody in (see `zeabur_api::local`).
