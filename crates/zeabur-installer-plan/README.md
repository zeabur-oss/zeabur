# zeabur-installer-plan

Planning for `zeabur-installer`: from a configuration, a Zeabur catalog release and a few host
facts to the exact files, objects and steps `apply` carries out. Nothing here touches the host
or the network, so every decision is unit-tested and `plan` is deterministic.
