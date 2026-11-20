import { defineConfig } from 'vite'
// @ts-expect-error The schema plugin runs only in the Node builder.
import { wireSchemas } from './schema-plugin.mjs'

export default defineConfig({
  base: '/ui/',
  plugins: [wireSchemas()],
  server: { proxy: { '/api': 'http://127.0.0.1:8181' } },
})
