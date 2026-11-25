---
default: patch
---

# Usage events survive errors and shutdown, and Zeabur Desktop sends them

Zeabur OSS, `zeaburctl ui` and Zeabur Desktop now queue the experience program's events in memory and send them in batches. A request that gets no answer, a rate limit or a server error is tried up to three more times, and each one waits a few seconds for the queue when it stops. Before, a failed request lost its event, any answer from the endpoint counted as sent, and events on their way when Zeabur OSS stopped were lost. Zeabur Desktop now sends `cluster_connected` as `zeaburctl ui` does, under the same setting: `zeaburctl telemetry disable`, `ZEABUR_TELEMETRY=off` or `DO_NOT_TRACK=1` turns it off. What is sent: https://oss.zeabur.com/docs/zeabur/reference/telemetry/
