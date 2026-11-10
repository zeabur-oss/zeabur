---
default: patch
---

# Registry credentials go only to the token service the registry names, over HTTPS

When Zeabur reads an image's ports and user, it sends the registry's username and password only to the token service in the registry's `WWW-Authenticate` challenge, and only over HTTPS. A token service on a loopback, link-local or private address is refused unless it is on the registry's own host. Redirects of registry requests carry no credentials or token, and a challenge after a redirect is not answered.
