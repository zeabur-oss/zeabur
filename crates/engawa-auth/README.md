# engawa-auth

How the in-cluster server signs people in through Genkan, keeps their sessions, and tells its
own page's requests from other sites'.

## Hides

- **The OAuth client** ([`client`]): engawa is a confidential client of Genkan (authorization
  code with PKCE S256, a client secret, `state`, an exact redirect URI), configured from
  `<issuer>/.well-known/openid-configuration`. It is written on the HTTP client stack kube-rs
  already ships (hyper, rustls with aws-lc-rs); there is no OpenID Connect or JOSE library, and
  no ID token is verified: who the person is comes from kube-apiserver (`SelfSubjectReview` with
  the access token), which validated the token itself.
- **The sign-in in progress** ([`flow`]): `state`, the PKCE verifier and where to go afterwards,
  sealed into a short-lived cookie, so nothing is stored for a visitor who has not signed in.
- **Sessions** ([`session`]): an opaque random identifier in a cookie; the tokens stay in this
  process's memory. Renewal shortly before expiry, one at a time per session; idle and absolute
  limits; administrator rights held for a few minutes.

## Place in the workspace

Level 1: depends on `engawa-core` only. Only the `engawa` binary uses it; `engawactl ui` signs
nobody in (see `engawa_api::local`).
