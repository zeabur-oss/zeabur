---
default: patch
---

# The restore form says what will exist afterwards

Restoring a database makes a second one, and the form now says so: the new database is created, the original keeps running, and services keep using it until you point them to the new one. The new database's page lists the next steps in order, with a link to delete the original.
