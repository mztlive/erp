/**
 * E2E 前端地址。与 scripts/ensure-services.sh 的端口约定保持一致：
 * - 默认（E2E_FRONTEND=prod）：生产构建 standalone server，端口 E2E_FRONT_PORT，默认 3100；
 * - E2E_FRONTEND=dev：复用 next dev，端口 3000。
 * E2E_BASE_URL 显式给出时优先。
 */
function defaultFrontendBaseUrl(): string {
    if (process.env.E2E_FRONTEND === "dev") return "http://localhost:3000"
    return `http://127.0.0.1:${process.env.E2E_FRONT_PORT ?? "3100"}`
}

export const FRONTEND_BASE_URL = process.env.E2E_BASE_URL ?? defaultFrontendBaseUrl()
