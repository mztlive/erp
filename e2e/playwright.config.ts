import { defineConfig } from "@playwright/test"

/**
 * ERP 一期 E2E 配置。
 *
 * 运行约定（见 scripts/run-flow.sh）：
 * - 每个流程（每个 spec 文件）通过 run-flow.sh 单独执行一次 playwright，
 *   执行前完成数据库 reset + 后端重启 + 审批定义发布；
 * - 数据库共享，因此 workers 固定为 1，禁止并行；
 * - 所有 flow-* 业务脚本使用默认页面串行切号，禁止为不同账号新建窗口；
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
            // headed 下 --start-maximized 最大化跟随实际窗口；headless 下最大化被忽略，
            // 窗口默认 800x600 会触发移动端壳布局（无「账号菜单」按钮），
            // 因此补 --window-size 保证 headless 也获得桌面壳尺寸。
            args: ["--start-maximized", "--window-size=1440,1000"],
            ...(Number.isFinite(slowMo) && slowMo > 0 ? { slowMo } : {}),
        },
    },
})
