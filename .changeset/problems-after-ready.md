---
default: patch
---

# Problems describe the pod as it is now

A service that failed its health checks while it started kept a red `健康檢查沒通過` banner and a `需要調整設定` tag on the canvas for up to 10 minutes after its pods were Ready. The Overview counted the same start-up warnings, so a fresh install showed warnings. Zeabur now leaves out an event about a pod from before the pod last became Ready. The service's and the project's event lists still show it. A probe that fails after the pod was Ready still shows the problem.
