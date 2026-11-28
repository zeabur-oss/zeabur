---
default: patch
---

# Put a service in a sidebar group from its Settings

A service's **Settings** has a **Group** field. It offers the project's groups, takes a new name, or takes the service out of its group when empty. The change sets or removes the `zeabur.com/group` label on the workload, so nothing restarts. The sidebar lists services by group as soon as one service has the label, with the others under **Ungrouped**. A chart's `app.kubernetes.io/component` label no longer makes a group. The service settings docs describe both the field and the label.
