---
default: major
---

# The server judges each service

The API now says what the service pages show, so every client reads the same conclusions. A service carries its `role` and a `verdict` with its headline and tone, its status says the `cause` in plain words, and a project's `health` names the `worst` one. A pod summary says its `cause`, a diagnosis says the `refusedPort` and a concrete fix, a Warning event says which `warning` it is, and a domain check says its `setup` and whether Let's Encrypt refused for its rate limit. `GET /projects/{project}/builds` says whether a build is under way, gives each run its `health`, and says when a build ran out of memory or the Git host refused its credentials. A service's reachability gives the ranges of its outgoing IP limit.
