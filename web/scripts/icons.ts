import { createRequire } from "node:module"
import { type Plugin, transform } from "@svgr/core"
import type { Options } from "unplugin-icons"

// `module.exports` is the plugin, but its types declare a `default` export, so tsc would type an
// import as the module.
const jsx = createRequire(import.meta.url)("@svgr/plugin-jsx") as Plugin

// unplugin-icons options for the build and the tests. This is its "jsx" compiler with svgr imported
// here: the built-in one loads @svgr/core by name from unplugin-icons, which the isolated linker
// does not let it see.
export const icons: Options = {
  scale: 1,
  compiler: {
    extension: "jsx",
    compiler: (svg, collection, icon) =>
      transform(
        svg,
        { plugins: [jsx], ref: true, titleProp: true },
        {
          componentName: `${collection}-${icon}`.replace(/-(\w)/g, (_, c: string) =>
            c.toUpperCase(),
          ),
        },
      ),
  },
}
