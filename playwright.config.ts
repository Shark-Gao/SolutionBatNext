import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: './tests/ui',
  outputDir: './test-results/browser',
  use: { baseURL: 'http://127.0.0.1:1420', channel: 'msedge', headless: true, screenshot: 'only-on-failure', viewport: { width: 1320, height: 920 } },
  webServer: { command: 'npm run dev', url: 'http://127.0.0.1:1420', reuseExistingServer: !process.env.CI, timeout: 30000 },
  timeout: 30000,
});
