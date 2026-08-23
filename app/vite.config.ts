import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Desktop-only app: Tauri serves the built assets, so no base-path juggling.
// Port is fixed because tauri.conf.json's devUrl points at it.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
})
