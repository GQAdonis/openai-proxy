import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import Ajv2020 from 'ajv/dist/2020.js'
import standaloneCode from 'ajv/dist/standalone/index.js'

// Generate the validators in the builder. The browser never compiles code or resolves a remote URI.
export function wireSchemas() {
  const name = 'virtual:console-wire-schemas'
  return {
    name: 'console-wire-schemas',
    resolveId(id) {
      if (id === name) return '\0' + name
    },
    load(id) {
      if (id !== '\0' + name) return
      const ajv = new Ajv2020({
        strict: false,
        code: { source: true, esm: true },
      })
      ajv.addFormat(
        'date-time',
        /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/i,
      )
      const ids = {}
      for (const file of [
        'common_types',
        'catalog',
        'agent_to_renderer',
        'renderer_to_agent',
      ]) {
        const path = resolve('src/schemas', file + '.json')
        this.addWatchFile(path)
        const schema = JSON.parse(readFileSync(path, 'utf8'))
        ajv.addSchema(schema)
        ids[file] = schema.$id
      }
      return standaloneCode(ajv, {
        validOperation: ids.agent_to_renderer,
        validAction: ids.renderer_to_agent,
      })
        .replaceAll('require("ajv/dist/runtime/equal").default', 'equal')
        .replace(
          /^/,
          'import equalModule from "ajv/dist/runtime/equal.js";const equal = equalModule.default ?? equalModule;\n',
        )
    },
  }
}
