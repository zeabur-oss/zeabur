---
default: patch
---

# A secret's YAML has no values, and any string can be set as a value

The YAML of a secret no longer carries `data` or `stringData`; its keys come beside it, and values are changed in the key editor. Editing a secret's YAML changes only the fields it shows and never removes or changes a key. Zeabur no longer treats `(hidden)` or a run of `•` as a value to skip: any string can be set on purpose.
