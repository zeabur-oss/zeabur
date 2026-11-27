---
default: patch
---

# A component install goes on after its dialog closes

The tab now runs a component install, not its dialog. **Hide** closes the dialog while the install goes on, a line at the bottom of every page shows the step it is on, and clicking it opens the dialog again. Closing or reloading the tab during an install asks first. On **Cluster** → **Backups**, a backup tool with a step not started shows "install not finished" instead of installed.
