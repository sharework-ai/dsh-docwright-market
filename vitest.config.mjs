import { defineConfig } from 'vitest/config'

// Coverage is configured via CLI flags in package.json's test:coverage script
// (all:false + include + thresholds): the coverage block of a config file was
// silently ignored by vitest 3.2.7 in this setup, while the same settings
// passed as CLI flags work. Keep the flags in one place — the script above.
export default defineConfig({
  test: {
    include: ['verifier/test/**/*.test.mjs', 'install.test.mjs', 'tests/**/*.test.mjs'],
    testTimeout: 30000,
  },
})
