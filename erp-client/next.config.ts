import type { NextConfig } from "next"
import path from "node:path"
import { fileURLToPath } from "node:url"

const projectRoot = path.dirname(fileURLToPath(import.meta.url))

const nextConfig: NextConfig = {
    output: "standalone",
    // 固定追踪根为 erp-client。上层若出现 lockfile，standalone 会改到
    // .next/standalone/erp-client/，镜像 COPY 会找不到 server.js。
    outputFileTracingRoot: projectRoot,
}

export default nextConfig
