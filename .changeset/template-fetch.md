---
default: minor
---

# Zeabur offers Zeabur Catalog's newest templates without an upgrade

When someone opens the template list, at most once every 6 hours, the Zeabur server fetches the newest release from `https://oss-catalog.zeabur.com/v1/`, checks that Zeabur Catalog's release workflow signed it, and offers its templates in place of the ones it was built with. A template that does not read, needs a component this Zeabur does not know, or does not make the objects the catalog recorded is not offered, and the built-in one stays. Components stay as built, and deployed services do not change. Turn it off with `templateFetch: false` in the installer's config, the chart's `templateFetch.enabled=false` or `ZEABUR_TEMPLATE_FETCH=false`; `DO_NOT_TRACK` does not turn it off. Point it at a mirror with `templateFetchUrl`, `templateFetch.url` or `ZEABUR_TEMPLATE_FETCH_URL`. See [Template updates](docs/reference/telemetry.mdx#template-updates).
