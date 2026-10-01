import { defineConfig } from "@playwright/test"

/** 只启动本地 HTTP 和 Chromium，验证隔离路由，不连接 ERP 或数据库。 */
export default defineConfig({
    testDir: "./helpers",
    testMatch: "**/*.test.ts",
    workers: 1,
    timeout: 30_000,
    reporter: "list",
    outputDir: "../logs/e2e-helper-self-tests",
    use: { headless: true, serviceWorkers: "allow" },
})
