import { defineConfig } from "@playwright/test"

/**
 * ERP 一期 E2E 配置。
 *
 * 运行约定（见 scripts/run-flow.sh）：
 * - 每个流程（每个 spec 文件）通过 run-flow.sh 单独执行一次 playwright，
 *   执行前完成数据库 reset + 后端重启 + 审批定义发布；
 * - 数据库共享，因此 workers 固定为 1，禁止并行；
 * - 用例内允许切换账号（每个账号一个独立 browser context）。
 */
export default defineConfig({
    testDir: "./tests",
    timeout: 240_000,
    expect: { timeout: 20_000 },
    fullyParallel: false,
    workers: 1,
    retries: 0,
    reporter: [["list"]],
    use: {
        baseURL: "http://localhost:3000",
        headless: true,
        screenshot: "only-on-failure",
        trace: "retain-on-failure",
        actionTimeout: 20_000,
        navigationTimeout: 30_000,
    },
})
