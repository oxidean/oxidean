/** @type {import('next').NextConfig} */
const nextConfig = {
  // Emit the self-contained server bundle the pack's Dockerfile ships.
  output: "standalone",
};

export default nextConfig;
