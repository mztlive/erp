/* eslint-disable no-console */
// 探测：复刻 flow-02 客户详情→上传合同按钮→合同页 路径，输出控制台诊断。
// 用法: npx tsx probes/probe-upload-flow.ts <customerId>
import { chromium } from "@playwright/test"
import { loginViaUi } from "../helpers/login"

const BASE = "http://localhost:3000"

async function main() {
    const customerId = process.argv[2]
    if (!customerId) throw new Error("需要 customerId 参数")
    const browser = await chromium.launch({
        headless: process.env.E2E_HEADED !== "1",
        args: ["--start-maximized"],
    })
    const context = await browser.newContext({ baseURL: BASE, viewport: null })
    const page = await context.newPage()
    page.on("console", (msg) => {
        const t = msg.text()
        if (t.startsWith("UPLOAD-")) console.log("[console]", t.slice(0, 500))
    })
    await loginViaUi(page, "sales")
    // 客户详情页（预热 customerKeys.detail 缓存，与测试一致）
    await page.goto(`/sales/customers/${customerId}`)
    await page.waitForTimeout(2500)
    // 点击"上传合同 PDF"（GuardedBusinessAction 渲染为 button）
    const btn = page.getByRole("button", { name: "上传合同 PDF" }).first()
    await btn.waitFor({ timeout: 20_000 })
    await btn.click()
    await page.waitForURL(/\/sales\/contracts\?customerId=/, { timeout: 20_000 })
    await page.waitForTimeout(4000)
    const dialog = page.getByRole("dialog").last()
    const submitBtn = dialog.getByRole("button", { name: "上传并归档" })
    console.log("submit disabled after open =", await submitBtn.isDisabled())
    // 填表（与测试一致）
    await dialog.getByLabel("合同编号").fill(`HT-PROBE-${Date.now().toString().slice(-6)}`)
    await dialog
        .locator('input[type="file"][aria-label="上传合同 PDF"]')
        .setInputFiles(require("node:path").join(__dirname, "../fixtures/sample-contract.pdf"))
    await page.waitForTimeout(1000)
    console.log("submit disabled after file =", await submitBtn.isDisabled())
    await browser.close()
}

main().catch((e) => {
    console.error("PROBE FAILED:", e)
    process.exit(1)
})
