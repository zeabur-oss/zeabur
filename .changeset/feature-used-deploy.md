---
default: patch
---

# Experience program counts how services, databases and domains are created

With the experience program on, Zeabur's server sends `feature_used` after a successful apply that creates a service (`git`, `upload`, `image` or a catalog `template`), a database or object storage (its kind), or a domain (`generated` or `custom`), at most once a day for each feature and set of values. See the Experience program page for what is sent.
