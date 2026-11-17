import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // Node's own runner executes the launcher tests in scripts/ (`node --test`, as CI does).
    exclude: ['tests/renderer/**', 'scripts/**/*.test.mjs', '**/node_modules/**', '**/.git/**'],
    // Real SQLite FULL-sync and native workers compete for the hosted Windows
    // volume. Serialize files, not the deliberately concurrent operations/peers
    // inside adversarial tests. Keep assertions and per-test deadlines unchanged.
    fileParallelism: process.platform !== 'win32',
  },
})
