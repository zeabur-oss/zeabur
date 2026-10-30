import { defineConfig } from "orval"

export default defineConfig({
  zeabur: {
    input: "../crates/zeabur-api/openapi.yaml",
    output: {
      target: "src/api/zeabur.ts",
      client: "fetch",
      mode: "single",
      urlEncodeParameters: true,
      override: {
        mutator: { path: "src/api/client.ts", name: "zeaburFetch" },
        fetch: { includeHttpResponseReturnType: false },
        useTypeOverInterfaces: true,
      },
    },
  },
})
