---
default: patch
---

# Logs say when the container has not started

The Logs tab of a service whose container has not started, such as one still waiting for its image, kept flashing between a loading placeholder and a message that the replica no longer exists. It now says the container has not started, and stays.
