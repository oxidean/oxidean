import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

/** @type {import('next').NextConfig} */
const nextConfig = {
  // Watt npm workspaces hoist `next` to the project root; point Turbopack there.
  turbopack: {
    root: resolve(dirname(fileURLToPath(import.meta.url)), '../..'),
  },
}

export default nextConfig
