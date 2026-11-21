---
default: patch
---

# Members show their passkeys again

The Members page showed `沒有通行密鑰` for every member, also for those who signed in with a passkey. Zeabur read the kind of a `Credential` from `spec.type`, which Zeabur Identity v0.3 no longer writes: the field that is set (`passkey`, `external` or `password`) names the kind. Zeabur now reads that shape. The passkey count, linked accounts, removing a credential or a member, an account reset and the device-bound passkey check for admin AAL3 work again.
