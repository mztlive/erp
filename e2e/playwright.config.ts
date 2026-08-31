import { defineConfig, devices } from "@playwright/test"

const slowMoRaw = process.env.E2E_SLOW_MO
const slowMo = slowMoRaw ? Number.parseInt(slowMoRaw, 10) : Number.NaN

/**
 * 流程 E2E 配置。服务启停与清库由 scripts/run-flow.sh 负责，这里不拉 webServer。
 * E2E_SLOW_MO 毫秒数写入 Chromium launchOptions.slowMo，供有界面慢动作观察。
 */
export default defineConfig({
    testDir: "./tests",
    testMatch: "**/*.spec.ts",
    fullyParallel: false,
    forbidOnly: Boolean(process.env.CI),
    retries: 0,
    workers: 1,
    timeout: 15 * 60 * 1000,
    expect: { timeout: 20_000 },
    reporter: [["list"], ["html", { open: "never" }]],
    outputDir: "test-results",
    use: {
        baseURL: process.env.E2E_BASE_URL ?? "http://localhost:3000",
        locale: "zh-CN",
        timezoneId: "Asia/Shanghai",
        viewport: { width: 1440, height: 900 },
        actionTimeout: 20_000,
        navigationTimeout: 30_000,
        trace: "retain-on-failure",
        screenshot: "only-on-failure",
        video: "retain-on-failure",
        launchOptions:
            Number.isFinite(slowMo) && slowMo > 0 ? { slowMo } : undefined,
    },
    projects: [
        {
            name: "chromium",
            use: { ...devices["Desktop Chrome"] },
        },
    ],
})
