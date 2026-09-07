import { defineConfig } from 'vitest/config';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  plugins: [vue()],
  server: { port: 1420, strictPort: true },
  clearScreen: false,
  test: { include: ['tests/**/*.test.ts'] },
});
