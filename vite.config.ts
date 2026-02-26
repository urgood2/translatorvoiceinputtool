import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

const __dirname = dirname(fileURLToPath(import.meta.url));

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],

  // Tauri configuration
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      // Ignore Rust source changes
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    // Tauri requires specific target for production builds
    target: process.env.TAURI_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
    // Output to dist for Tauri to consume
    outDir: 'dist',
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
        overlay: resolve(__dirname, 'overlay.html'),
      },
    },
  },
});
