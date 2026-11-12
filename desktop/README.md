# Zeabur desktop app (proof of concept)

A Tauri 2 app that lists every context of every kubeconfig on the machine and opens Zeabur's web
interface for the ones you pick. Throwaway code to answer product questions; not built in CI.

```sh
nix develop -c desktop/run.sh             # your own kubeconfig
nix develop -c desktop/run.sh --fixtures  # the sample kubeconfigs in desktop/fixtures
```

`run.sh` starts the demo API (`cargo run -p zeabur-api --example demo`), the web dev server on
port 5173, and the app. The chooser is `web/desktop.html` with `web/src/desktop/`; the Rust side is
`desktop/src/`.

## What it does

- Reads `KUBECONFIG` (`:` between paths, `;` on Windows) or `~/.kube/config`
  (`%USERPROFILE%\.kube\config` on Windows) with the same rules as kubectl, through
  `std::env::split_paths` and kube-rs `Kubeconfig::read_from`.
- Sources: the default paths, plus files, folders (every non-hidden file in it), drag and drop, and
  pasted YAML. Pasted YAML is checked, then saved under the app's data folder. Any source can be
  removed; removing a default only hides it. The app never writes to a kubeconfig.
- Each file is parsed on its own. A context name used in two files shows twice, each with its
  file, and says how many others share the name.
- Each row: name, file, server, user, a reachability badge (a TCP connect to the server, 1.5 s
  timeout, run in parallel) and an auth check. Contexts whose user or cluster is missing, whose
  certificate file is missing, or whose exec plugin is not installed are shown in red and cannot
  be opened. An exec plugin found only on the login shell's PATH is shown in amber and can be opened.
- Search over name, server, user and file; filter by source; favourites (star, sorted first);
  the last five opened as "Recently opened"; select several and open them together. Each cluster
  opens in its own window.
- English, Traditional Chinese and Simplified Chinese from Fluent files in
  `web/src/desktop/locales/`. The first language comes from the system; the picker in the sidebar
  switches it live.

What is mocked: every "Open" loads the same web URL (`ZEABUR_DESKTOP_WEB`, default the dev server)
against the demo API, so every window shows the demo cluster. A real app would start a Zeabur
backend for the chosen context. Signing in to the cluster is not attempted; reachability is only
a TCP connection.

## Screenshots

`desktop/shots/<lang>-<screen>-<theme>.png` (made by `desktop/shots.sh`), 4 languages × light and dark, taken from the running
app (a WKWebView snapshot of the window, no screen recording):

| Screen    | Shows                                                                  |
| --------- | ---------------------------------------------------------------------- |
| `first`   | No kubeconfig found: choose files, a folder, paste, or drop            |
| `list`    | Two sources, a favourite, recently opened, every kind of problem      |
| `search`  | "staging" found in two files                                           |
| `paste`   | The paste dialog with a sample kubeconfig                              |
| `select`  | Two contexts selected, "Open 2 clusters"                               |
| `drop`    | A file dragged over the window                                         |
| `cluster` | The window opened for `zeabur-demo`                                    |

The shots run with the fixtures and a Finder-like environment (`PATH=/usr/bin:/bin:/usr/sbin:/sbin`,
no shell variables), so `aws` is found only through the login shell. Every capture has a 28 pt band
at the bottom: WebKit reports a 28 pt top safe area (the title bar) and lays the page out 28 pt
shorter than the view.

## How others do it

Kubernetes itself: `KUBECONFIG` is a list of files, `:`-separated on Linux and macOS and
`;`-separated on Windows; unset, kubectl reads `$HOME/.kube/config`. When files are merged, the first
file to set a key wins and a later entry with the same name is dropped. `--kubeconfig` uses one file
with no merge.[^k8s]

Aptakube:

- Zero setup: "Open the app and your clusters appear"; the first screen lists every context in the
  kubeconfig.[^apta-zero][^apta-multi] Since v1.19.0 it reconnects to the last cluster on launch;
  v1.19.4 added a setting to show the selector instead.[^apta-log]
- Sources resolve as `$APTAKUBECONFIG`, then `$KUBECONFIG`, then `~/.kube/config`, each a
  colon-separated list. Preferences > Kubeconfig Sources offers Add Files, Add Folder and Enter
  Path, plus a Merge Files setting for configs split across files.[^apta-ctx]
- It never writes to kubeconfig files. Alias, icon, colour, tags and change protection live in
  its own settings,[^apta-zero][^apta-ctx] or in an `aptakube` extension inside the kubeconfig so a
  team can share them, with an `order` field for sorting.[^apta-ext] Aliases change the name only
  inside the app.[^apta-log]
- Favourites on F1-F12 (v1.19.0), status indicators per connection (v1.18.5), Cmd/Ctrl+O for the
  context selector (v1.22.0), Shift+Click on Switch Context for a new window (v1.21.5).[^apta-log]
- Several contexts open together as one merged table with a coloured Context column, and a tag
  such as `#production` opens a group in one click.[^apta-multi]
- When an EKS, GKE or AKS sign-in expires, a "Sign in with …" button runs the login without a
  terminal (v1.21.6).[^apta-log]

Lens reads `~/.kube/` automatically and can add a kubeconfig by pasting it, which it copies into
`~/.lens/kubeconfigs`; adding from disk is one file at a time.[^lens] Freelens finds exec plugins
by starting the login shell (`bash -l`) and reading its environment; that misses some setups
(asdf, mise, WSL, Flatpak), and the suggested fix is an absolute path in the kubeconfig.[^fl-1057]
A failed shell sync used to show only in the log; it now shows in the UI.[^fl-2727] Passing paths
through `KUBECONFIG` broke a file name containing `:`, fixed by passing `--kubeconfig`.[^fl-2895]
Headlamp takes extra kubeconfigs as a command-line argument or `KUBECONFIG`, `:` on Unix and `;` in
PowerShell,[^hl-desk] and has an open report of an exec-based cluster that works from a terminal
but not from Finder or the Dock.[^hl-1716] k9s switches with `:ctx` and keeps a per-context
`readOnly` setting.[^k9s-cmd][^k9s-cfg]

### Where this PoC follows Aptakube and where it does not

Follows: every context from every source on the first screen with no setup; sources as files,
folders and paths; never writing to kubeconfig; favourites; status per context; opening several.

Differs on purpose:

- No merge. Each file is read alone, so a context name in two files is two rows, each with its
  file. Merging (kubectl's rule) would silently drop the second one; a Merge Files setting like
  Aptakube's can be added for split configs.
- Paste and drag and drop, which Aptakube's docs do not mention. Pasted YAML is copied into the
  app's folder (as Lens does), so the original can be deleted.
- Broken contexts are listed and marked instead of failing on open, with the reason in the row.
- Several clusters open as separate windows, not one merged table. Zeabur's web interface is one
  cluster per page today.
- Exec plugins found only on the login shell's PATH are marked, so the person knows why it would
  fail from Finder before opening it.

## Decisions for the owner

1. **Sources.** Keep the sidebar list (default, files, folders, pasted) with a Merge Files toggle,
   or follow kubectl and merge everything? The PoC reads each file alone.
2. **Exec plugins and PATH.** A Finder or Dock launch gets `/usr/bin:/bin:/usr/sbin:/sbin`. The PoC
   runs `$SHELL -l -c 'printf %s "$PATH"'` once at start and searches both. Options: keep that,
   add a PATH setting, or ask for absolute paths. Login shells can be slow or print to stdout;
   a timeout is not in the PoC.
3. **One window or several.** The PoC opens one window per cluster (Aptakube merges them into one
   table). Several windows match the web interface today but mean several backends.
4. **What "Open" starts.** Embed the Zeabur backend in the app and run one per context, or point at
   a backend already running in the cluster. Decides install size and how sign-in works.
5. **Updates and signing.** Tauri's updater needs a signed update feed; macOS needs a Developer ID
   certificate and notarization; Windows needs a code signing certificate to avoid SmartScreen
   warnings. None of this is set up.
6. **The word "context" in Chinese.** zh-TW and zh-CN keep "context" (kubectl's word).

## Windows

Checked by reading code and config, not by running on Windows:

- `KUBECONFIG` is split with `std::env::split_paths`, which uses `;` on Windows; a unit test covers
  `C:\Users\a\.kube\config;D:\kube\team.yaml;` on Windows and the `:` form elsewhere.
- The default path is `home_dir()\.kube\config`, which is `%USERPROFILE%\.kube\config`.
- Exec plugins are also looked up as `.exe`, `.cmd` and `.bat`; the login shell lookup is skipped.
- Tauri 2 on Windows uses WebView2, which ships with Windows 11; `tauri.conf.json` has no
  Windows-specific settings, and `icons/icon.ico` is there for the executable's icon.
- The screenshot hook is macOS only.

See the bottom of this file for what was built on Windows.

[^k8s]: https://kubernetes.io/docs/concepts/configuration/organize-cluster-access-kubeconfig/
[^apta-zero]: https://aptakube.com/zero-config
[^apta-multi]: https://aptakube.com/multi-cluster
[^apta-ctx]: https://aptakube.com/docs/contexts
[^apta-ext]: https://aptakube.com/docs/context-extension
[^apta-log]: https://aptakube.com/changelog
[^lens]: https://docs.lenshq.io/k8slens/getting-started/add-clusters/add-local-cluster/
[^fl-1057]: https://github.com/freelensapp/freelens/issues/1057
[^fl-2727]: https://github.com/freelensapp/freelens/issues/2727
[^fl-2895]: https://github.com/freelensapp/freelens/issues/2895
[^hl-desk]: https://headlamp.dev/docs/latest/installation/desktop/
[^hl-1716]: https://github.com/kubernetes-sigs/headlamp/issues/1716
[^k9s-cmd]: https://k9scli.io/topics/commands/
[^k9s-cfg]: https://k9scli.io/topics/config/
