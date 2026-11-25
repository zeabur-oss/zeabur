---
default: patch
---

# A refused request says why the cluster refused it

A refused change no longer says the object changed after you reviewed it unless it did. A name that is already taken now says it already exists, an expired watch or list says it expired, and other conflicts say what the cluster answered. Only a change whose objects moved on after the review says so and opens the review again. Removing a component that other components need now names them in your language and asks you to remove them first.
