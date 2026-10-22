import { defineConfig } from "vite";
import { ripple } from "@ripple-ts/vite-plugin";

export default defineConfig({
  plugins: [ripple()],
});
