import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// In dev, API calls go to shotclock-web running on its default port.
export default defineConfig({
  plugins: [react()],
  server: { proxy: { "/api": "http://127.0.0.1:7777" } },
});
