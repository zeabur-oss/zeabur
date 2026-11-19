# Contracts

What Zeabur relies on from Zeabur Identity and Zeabur Builder, as each project publishes it.
Zeabur pins no version of either: the tests check Zeabur against these files, and a change on
either side shows up here as a diff in a pull request.

| File | From | Read by |
|---|---|---|
| `identity/crds.json` | Zeabur Identity's `deploy/crds.json` (`zeabur-identityctl crds`) | every `identity.zeabur.com` object in a fixture, a plan or an API test write |
| `identity/openid-configuration.json` | Zeabur Identity's discovery document, with the issuer `https://identity.example` | the issuer of `zeabur-auth`'s tests, which serves it |
| `identity/token.json` | the fields of Zeabur Identity's token answer; the values are placeholders | the token answers of that issuer |
| `builder/crds.yaml` | Zeabur Builder's `deploy/chart/crds/crds.yaml` | every `builder.zeabur.com` object in a fixture, a plan or an API test write |

`zeabur-test-support`'s `contracts` checks an object against its CRD's schema: a field the schema
does not have, a value of another type or outside an enum, and, for an object Zeabur writes, a
missing required field, fail the test.

To take a new release of either project, copy its files here unchanged and run the tests. A
failure is a contract change: change Zeabur, or ask the other project to keep what Zeabur uses.
