---
default: major
---

# Zeabur and zeabur-proxy no longer take flags for session lengths and proxy limits

`zeabur --session-idle-minutes` and `--session-max-hours`, and `zeabur-proxy --timeout`, `--max-bytes`, `--review-cache`, `--max-inflight` and `--max-inflight-per-person`, with their `ZEABUR_SESSION_*` and `ZEABUR_PROXY_*` environment variables, are removed. The values stay the same: a session ends after one hour idle or twelve hours in all, and the proxy keeps its 30-second timeout, 16 MiB request limit, 10-second review cache, 16 requests at a time and 2 per person. The chart and the installer never set them. If you added one to a Deployment by hand, delete it from its `args` and `env`.
