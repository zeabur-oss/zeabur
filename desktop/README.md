# Zeabur Desktop

A Tauri 2 app that lists every context of every kubeconfig on the machine and opens Zeabur for the
ones you pick. Each cluster window runs its own local service (`zeabur-local`, the same one
`zeaburctl ui` runs) on `127.0.0.1` and loads it, so the app needs no other program and no
environment variables. It is a Cargo workspace of its own with the main workspace's rules
([crates](../docs/internal/crates.md#apps-outside-the-workspace)); CI's `desktop` job lints and
tests it on Linux.

```sh
nix develop .#desktop -c desktop/run.sh             # your own kubeconfig
nix develop .#desktop -c desktop/run.sh --fixtures  # the sample kubeconfigs in desktop/fixtures
```

`run.sh` builds the web interface into `web/dist` when it is missing, starts the dev server of the
app's own pages on port 5174, and the app. The app's own pages are in `desktop/web/` (Vite): the
chooser (`index.html`, `src/chooser/`) and the page a cluster window shows while its service
starts or when it cannot (`cluster.html`, `src/cluster/`). The Rust side is `desktop/src/`.

## Build the app

```sh
nix develop .#desktop -c scripts/build-desktop.sh
```

It builds `web/dist` (embedded in the local service), `desktop/web/dist` (bundled into the app),
the binary, then the bundles: on macOS `Zeabur.app` and `Zeabur_<version>_<arch>.dmg` under
`target/desktop/release/bundle/`. The version is `web/package.json`'s. A binary built in the Nix
shell links the Nix store's libiconv; on macOS the script points it at `/usr/lib/libiconv.2.dylib`
before the bundler signs it, and `scripts/check-portable.sh` then fails if anything in the app
links a library that a Mac without Nix lacks. CI's `desktop-bundle` job does the same on macOS.
On Linux the check fails for now: a binary built in the Nix shell uses the Nix store's dynamic
loader, glibc and WebKitGTK, so a `.deb` or AppImage of it would not run on other distributions. With
`APPLE_SIGNING_IDENTITY` set to a Developer ID Application identity the bundler signs with the
hardened runtime; with `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_PATH` (or
`APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID`) it also notarizes. Without them the app is signed
ad hoc, and macOS asks before the first launch of a downloaded copy.

## What it does

- Default kubeconfig: the app's own `KUBECONFIG`, else the login shell's `KUBECONFIG`, else
  `~/.kube/config` (`%USERPROFILE%\.kube\config` on Windows). `KUBECONFIG` is a list (`:` between
  paths, `;` on Windows), split with `std::env::split_paths` and read with kube-rs
  `Kubeconfig::read_from`.
- Sources: the default paths, plus files, folders (every non-hidden file in it), drag and drop, and
  pasted YAML. Files and folders are read in place and stored as absolute paths. Pasted YAML is
  checked, then copied into the app's `kubeconfigs` folder. Any source can be removed; removing a
  default only hides it. The app never writes to a kubeconfig.
- Each file is parsed on its own. A context name used in two files shows twice, each with its
  file, and says how many others share the name.
- Each row: name, file, server, user, a reachability badge (a TCP connect to the server, 1.5 s
  timeout, run in parallel) and an auth check. Contexts whose user or cluster is missing, whose
  certificate file is missing, or whose exec plugin is not installed are shown in red and cannot
  be opened.
- Search over name, server, user and file; filter by source; favourites (star, sorted first);
  the last five opened as "Recently opened". One cluster opens at a time, in its own window.
- English, Traditional Chinese and Simplified Chinese (`desktop/web/src/i18n`). The first
  language comes from the system; the picker in the sidebar switches it live, and a cluster window
  opens in the same language.

## Folders

The identifier is `com.zeabur.oss.desktop`. Every folder comes from Tauri's `PathResolver`, which
adds the identifier:[^tauri-path]

| What | API | macOS | Linux | Windows |
| --- | --- | --- | --- | --- |
| `settings.json`: added sources, hidden defaults, favourites, recently opened | `app_config_dir` | `~/Library/Application Support/` | `$XDG_CONFIG_HOME` or `~/.config/` | `%APPDATA%\` |
| Pasted kubeconfigs | `app_local_data_dir/kubeconfigs` | `~/Library/Application Support/` | `$XDG_DATA_HOME` or `~/.local/share/` | `%LOCALAPPDATA%\` |

Pasted kubeconfigs hold credentials: they stay out of the roaming folder on Windows, and on macOS
and Linux the folder is `0700` and each file `0600`. `ZEABUR_DESKTOP_STATE` puts both in one
folder; `run.sh --fixtures` and `shots.sh` use it.

## Exec plugins and the login shell

An app started from Finder, the Dock or a desktop launcher does not get the variables set in shell
files, so `aws`, `gcloud` or `kubelogin` installed through `~/.zshrc` are not on its `PATH`. On
macOS and Linux the app reads the login shell's environment once, in the background, and the
window reloads when it is done:

- The shell is the passwd entry's, else `$SHELL`, else `/bin/zsh` on macOS and `/bin/sh` on Linux.
- zsh, bash and sh run with `-l -i -c` (zsh reads `~/.zshrc` only when interactive), fish with
  `-l -c`. The command prints a marker, `/usr/bin/env -0` and the marker again, so text that shell
  files print before or after is ignored. `DISABLE_AUTO_UPDATE=true` stops Oh My Zsh from asking
  about updates; stdin is empty; the timeout is 10 seconds.
- The result is one environment for child processes: the app's own variables win, except `PATH`,
  which lists the shell's directories first and then the app's, each once. It is passed with
  `Command::envs` every time; `std::env::set_var` is not used, as it is unsound in a
  multi-threaded program.[^set-var] On Unix, `Command` looks the program up on the `PATH` it is
  given.[^command]
- The "is the exec plugin installed?" check uses the same environment, so a context that the list
  shows as ready finds its plugin when it runs.
- If the shell fails or times out, the window says so and suggests an absolute path for the exec
  command in the kubeconfig.

On Windows the app uses its own environment, with no shell, and tries each extension in `PATHEXT`
for a plugin name without one.

## Opening a cluster

- "Open" makes a window for the context and loads `cluster.html`, which asks the app to start the
  local service for that file and context (`zeabur_local::start_from`). The service signs in with
  the kubeconfig, listens on `127.0.0.1` on a port the system picks, and the window loads its
  one-time link. Only that window's first request gets a session (`zeabur_api::local`).
- An exec credential plugin gets the login shell's variables (`PATH` first) through the
  kubeconfig's exec `env`, unless the entry sets them itself. The file is read again on every
  open, and the app never writes it.
- If the service cannot start (the kubeconfig cannot sign in, the cluster does not answer who you
  are), the window shows the reason with "Try again" and "Choose another cluster".
- Closing the window stops its service. Quitting the app stops them all.
- On macOS a cluster on the local network (a homelab, a VM) needs the Local Network permission.
  macOS asks on the first connection, with the reason in `desktop/Info.plist`; until it is
  allowed the connection fails with "No route to host".
- The chooser's reachability badge is still only a TCP connection; signing in happens on open.
- `ZEABUR_DESKTOP_WEB=<url>`, for development only, makes every cluster window load that address
  (such as the web dev server with the demo API) instead of starting a service.

## Security

- The app's own pages run under a strict content security policy (`tauri.conf.json`): scripts and
  styles from the app only (inline styles allowed, as in the web interface), no network but the
  app's IPC.
- Every command needs a permission (`build.rs`, `capabilities/`). The chooser may call the
  chooser's commands; a cluster window's own page may call `start_cluster` and
  `back_to_chooser`. The web interface on `127.0.0.1` is a remote page and may only move and
  zoom the window on macOS (`chrome.rs`).

## Windows and menus

- Every cluster is its own window. A new one is placed a little below and right of the last; each
  window's size, place and maximized state are kept between launches (`window-state.json`, in
  `ZEABUR_DESKTOP_STATE` when set).
- macOS: the title bar is drawn over the page, as in Finder and Notes. The page leaves 28 pt at the
  top (`data-titlebar` and `--zeabur-title-bar`, set by `desktop/src/chrome.rs`) and a strip there
  moves the window. The menu bar (`desktop/src/menu.rs`) uses the `desktop.menu.*` strings and
  changes with the language picker. In Chinese, the standard items use the words of the macOS 27
  menus (Notes and Safari, looked up on [applelocalization.com](https://applelocalization.com/)),
  such as 拷貝 and 拷贝 for Copy. The items macOS adds itself (Fill, Center, Move & Resize, Writing
  Tools, …) follow the system language, in English, Traditional or Simplified Chinese
  (`desktop/Info.plist`), not the picker. The app stays open with no windows; a click on the Dock icon
  or File > New Cluster Window shows the chooser again.
- Windows and Linux: the system title bar, which follows the light or dark system theme, no menu
  bar, and the app quits when the last window closes. A new window is at most 90% of the screen's
  work area, so it fits at 125% or 150% scaling.
- Linux (GNOME): the window handles the standard shortcuts before the page sees them[^gnome-keys].
  Ctrl+W closes the window, Ctrl+Q quits, Ctrl+N shows the chooser, Ctrl+R reloads, and Ctrl++,
  Ctrl+- and Ctrl+0 zoom. The program name is the app ID (`com.zeabur.oss.desktop`), which Wayland
  uses to find the desktop entry[^xdg-shell]. `desktop/run.sh` writes that entry and the icon (`hicolor`
  theme, 32 to 512 px) while it runs, so the Dock and Alt+Tab show Zeabur and its icon. GNOME maximizes a new window that is nearly the size of the
  screen.
- Windows: Ctrl+W closes the window and Ctrl+N shows the chooser[^win-keys], through WebView2's
  `AcceleratorKeyPressed` event, so the page never sees them. WebView2 handles Ctrl+R and F5, and
  Ctrl++, Ctrl+- and Ctrl+0 zoom. Alt+F4 closes the window. The process sets the AppUserModelID
  `com.zeabur.oss.desktop` before the first window[^win-appid], so the taskbar keeps every window
  under one Zeabur button. A window is placed inside the work area, title bar included, and a cluster
  window can be made 500 effective pixels wide, the most Snap layouts allows for a minimum[^win-snap].

## Icon

`icons/logo-for-dark-bg.svg` is the mark from Zeabur's logo pack, unchanged. `icons/icon.svg` puts it,
504 px wide, on the 824 px rounded square of Apple's 1024 px macOS icon grid, in the logo's own dark
(`#141216`). A white square loses the dark stroke of the Z when macOS shows icons in its dark style.
The other files in `icons/` come from it:

```sh
cargo tauri icon desktop/icons/icon.svg -o desktop/icons  # then delete android/, ios/, 64x64.png, Square*.png, StoreLogo.png
```

## Screenshots

`desktop/shots/<lang>-<screen>-<theme>.png` (made by `desktop/shots.sh`, not committed), 4 languages × light and dark, taken from the running
app (a WKWebView snapshot of the window, no screen recording):

| Screen    | Shows                                                                  |
| --------- | ---------------------------------------------------------------------- |
| `first`   | No kubeconfig found: choose files, a folder, paste, or drop            |
| `list`    | Two sources, a favourite, recently opened, every kind of problem      |
| `search`  | "staging" found in two files                                           |
| `paste`   | The paste dialog with a sample kubeconfig                              |
| `drop`    | A file dragged over the window                                         |
| `cluster` | The window opened for `zeabur-demo`, which cannot sign in: the error page |

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

### Where the app follows Aptakube and where it does not

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
- A failed login shell read is shown in the window with what to do, as Freelens now does, instead
  of only in a log.

## Decisions for the owner

1. **Sources.** Keep the sidebar list (default, files, folders, pasted) with a Merge Files toggle,
   or follow kubectl and merge everything? The app reads each file alone.
2. **One window or several.** The app opens one window per cluster (Aptakube merges them into one
   table). Several windows match the web interface today but mean several backends.
3. **Updates and signing.** Tauri's updater needs a signed update feed, and Windows needs a code
   signing certificate to avoid SmartScreen warnings. Neither is set up. `scripts/build-desktop.sh`
   signs and notarizes the macOS app when the signing variables are set; no CI job does it.
4. **The word "context" in Chinese.** zh-TW and zh-CN keep "context" (kubectl's word).

## Windows

Build for Windows from macOS or Linux with [cargo-xwin](https://github.com/rust-cross/cargo-xwin),
which downloads the MSVC CRT and Windows SDK on first use (needs `clang`, `lld` and `llvm`):

```
cd desktop
cargo xwin build --release --target aarch64-pc-windows-msvc   # or x86_64-pc-windows-msvc
cargo xwin clippy --all-targets --all-features --locked --target aarch64-pc-windows-msvc -- -D warnings
```

CI does not build for Windows or macOS; run `cargo lint` on a Mac and the `clippy` line above
after a change to Windows or macOS code.

Copy `target/<target>/release/zeabur-desktop.exe` to a Windows 11 machine and run it; WebView2 ships
with Windows 11. A release build is a GUI program with no console window; a debug build keeps the
console for its log.

Run on Windows 11 ARM64 (build 26200) at 100%, 150% and 200% scaling, light and dark:

- Windows open inside the work area and come back maximized or at their last size and place.
- Ctrl+W closes the focused window, and the process exits with the last one. Ctrl+N brings the
  chooser forward.
- The title bars and pages follow a switch between light and dark while open. A change of scaling
  while open redraws the window sharp at the new size.
- Hovering the maximize button shows Snap layouts. The taskbar shows one Zeabur button with the
  icon, and its jump list names Zeabur.

From code and unit tests only:

- `KUBECONFIG` is split with `std::env::split_paths`, which uses `;` on Windows; a unit test covers
  `C:\Users\a\.kube\config;D:\kube\team.yaml;` on Windows and the `:` form elsewhere.
- The default path is `home_dir()\.kube\config`, which is `%USERPROFILE%\.kube\config`.
- Exec plugins are also looked up with each extension in `PATHEXT` (`.COM;.EXE;.BAT;.CMD` when it
  is unset); the login shell read is skipped.
- The screenshot hook is macOS only.

Not done: an installer and Start menu shortcut, Mica behind the page, the accent colour on title
bars, and high contrast themes.

[^tauri-path]: https://docs.rs/tauri/latest/tauri/path/struct.PathResolver.html
[^set-var]: https://doc.rust-lang.org/std/env/fn.set_var.html
[^command]: https://doc.rust-lang.org/std/process/struct.Command.html
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
[^gnome-keys]: https://developer.gnome.org/hig/reference/keyboard.html
[^xdg-shell]: https://wayland.app/protocols/xdg-shell#xdg_toplevel:request:set_app_id
[^win-keys]: https://learn.microsoft.com/en-us/windows/apps/develop/input/keyboard-accelerators
[^win-appid]: https://learn.microsoft.com/en-us/windows/win32/shell/appids
[^win-snap]: https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-snap-layout-menu
