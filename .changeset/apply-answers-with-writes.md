---
default: major
---

# Apply answers with its writes, and the deploy key is made at apply

`POST /components/{id}/apply` now answers with the plan and the result of each write, so a client sees which writes landed. A Git service's deploy key is now made when the service is applied: the Git drawer shows its public key after the service is created, and `createDeployKey` is replaced by `inspectDeployKeyHost`, which only checks the Git host. Removing a component now sends the objects it reviewed as preconditions, so a change made in the meantime stops the removal. Clients of the old answers must read the new shapes.
