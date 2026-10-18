import type { NextConfig } from "next";

const isProd = process.env.NODE_ENV === "production";
const internalHost = process.env.TAURI_DEV_HOST || "localhost";

const nextConfig: NextConfig = {
  // Tauri loads a static export; there is no Next.js server at runtime.
  // https://v2.tauri.app/start/frontend/nextjs/
  output: "export",
  // next/image optimization requires a server; serve images as-is.
  images: { unoptimized: true },
  // In dev, the webview loads assets from the Next dev server.
  assetPrefix: isProd ? undefined : `http://${internalHost}:3100`,
};

export default nextConfig;
