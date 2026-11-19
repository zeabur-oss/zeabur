---
default: minor
---

# Zeabur offers Zeabur Catalog's newest templates without an upgrade

When someone opens the template list, at most once every 6 hours, the Zeabur server reads `catalog.json` from Zeabur Catalog's API at `https://oss-catalog.zeabur.com/v1/`, fetches `templates/<id>.json` for each template that changed, and offers those templates in place of the ones it was built with. Zeabur trusts the HTTPS address it fetches from; the files carry no signature. A template that does not read or needs a component this Zeabur does not know is not offered, and the one offered before stays. Components stay as built, and deployed services do not change. Turn it off with the chart's `templateFetch.enabled=false` or `ZEABUR_TEMPLATE_FETCH=false`; `DO_NOT_TRACK` does not turn it off. It reads the same address as component installs, the chart's `catalog.url` or `ZEABUR_CATALOG_URL`, so one setting points both at a mirror. See [Template updates](docs/reference/telemetry.mdx#template-updates).
