title = Choose a cluster
search = Search by name, server, user or file
sources = Kubeconfig sources
source-all = All contexts
source-default = Default
source-file = File
source-folder = Folder
source-pasted = Pasted
source-remove = Remove { $name }
source-missing = File not found
source-errors = { $n ->
    [one] 1 file can't be read
   *[other] { $n } files can't be read
}
add-source = Add a source
add-files = Choose files
add-folder = Choose a folder
add-paste = Paste a kubeconfig
drop = Drop kubeconfig files or folders to add them
first-title = No kubeconfig yet
first-body = Zeabur looks for { $path }. Add a kubeconfig file, a folder of them, or paste one.
first-drop = You can also drop files or folders here.
col-context = Context
col-server = Server
col-user = User
col-status = Status
recent = Recently opened
favourites = Favourites
count = { $n ->
    [one] 1 context
   *[other] { $n } contexts
}
no-results = No context matches “{ $q }”
current = Current
same-name = { $n ->
    [one] 1 more with this name
   *[other] { $n } more with this name
}
reach-checking = Checking…
reach-ok = Reachable
reach-unresolved = Unknown host
reach-refused = Connection refused
reach-timeout = No answer
reach-none = No server address
auth-shell-path = Uses { $command } from your shell's PATH
auth-plugin-missing = { $command } isn't installed
auth-missing-user = User { $name } isn't in this file
auth-missing-cluster = Cluster { $name } isn't in this file
auth-file-missing = Can't find { $path }
reach-note = Reachable means the server's address answers. Signing in happens when you open it.
open = Open
open-n = { $n ->
    [one] Open 1 cluster
   *[other] Open { $n } clusters
}
selected = { $n ->
    [one] 1 selected
   *[other] { $n } selected
}
clear = Clear
favourite-add = Add to favourites
favourite-remove = Remove from favourites
paste-title = Paste a kubeconfig
paste-name = Name
paste-yaml = Kubeconfig
paste-add = Add kubeconfig
paste-error = Zeabur can't read this kubeconfig: { $reason }
paste-empty = This kubeconfig has no contexts.
cancel = Cancel
language = Language
