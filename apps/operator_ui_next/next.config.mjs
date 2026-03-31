import path from "node:path";

/** @type {import('next').NextConfig} */
const nextConfig = {
  reactStrictMode: true,
  output: "standalone",
  outputFileTracingRoot: path.join(import.meta.dirname, "../../"),
  
  // Optimize package imports for smaller bundles
  experimental: {
    optimizePackageImports: ['lucide-react', 'class-variance-authority'],
  },
  
  // Production optimizations
  productionBrowserSourceMaps: false,
  
  // Enable compression
  compress: true,
};

export default nextConfig;
