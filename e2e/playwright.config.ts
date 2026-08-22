import { defineConfig } from "@playwright/test"

/**
 * ERP 一期 E2E 配置。
 *
 * 运行约定（见 scripts/run-flow.sh）：
 * - 每个流程（每个 spec 文件）通过 run-flow.sh 单独执行一次 playwright，
 *   执行前完成数据库 reset + 后端重启 + 审批定义发布；
 * - 数据库共享，因此 workers 固定为 1，禁止并行；
 * - 用例按流程合同选择独立 browser context 或单页面串行切号；
 * - headed 浏览器启动即最大化，页面 viewport 跟随实际窗口。
 * - `E2E_SLOW_MO` 不是 Playwright Test CLI 参数，只能从这里读入 launchOptions.slowMo。
 */
const slowMo = Number(process.env.E2E_SLOW_MO)

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
        viewport: null,
        screenshot: "only-on-failure",
        trace: "retain-on-failure",
        actionTimeout: 20_000,
        navigationTimeout: 30_000,
        launchOptions: {
            args: ["--start-maximized"],
            ...(Number.isFinite(slowMo) && slowMo > 0 ? { slowMo } : {}),
        },
    },
})
