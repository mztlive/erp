/* eslint-disable no-console */
// 探测：合同上传对话框"上传并归档"禁用原因（flow-02 复现）。
// 用法: npx tsx probes/probe-contract-upload.ts
import { chromium } from "@playwright/test"
import * as path from "node:path"
import { loginViaUi } from "../helpers/login"

const BASE = "http://localhost:3000"
const CUSTOMER_ID = "9fe1551daa954dbf88bd73be42c3cc6a" // flow-02 上次失败运行创建的客户
const CUSTOMER_NAME = "e2e代发客户53788099"
const CONTRACT_NO = `HT-PROBE-${Date.now().toString().slice(-6)}`

async function main() {
    const browser = await chromium.launch({
        headless: process.env.E2E_HEADED !== "1",
        args: ["--start-maximized"],
    })
    const context = await browser.newContext({ baseURL: BASE, viewport: null })
    const page = await context.newPage()
    await loginViaUi(page, "sales")
    await page.goto(`${BASE}/sales/contracts?customerId=${CUSTOMER_ID}`)
    const dialog = page.getByRole("dialog").last()
    await dialog.getByRole("heading", { name: "上传合同 PDF" }).waitFor({ timeout: 30_000 })

    // 1) 与 flow-02 测试相同的操作序列
    await dialog.getByLabel("合同编号").fill(CONTRACT_NO)
    await dialog
        .locator('input[type="file"][aria-label="上传合同 PDF"]')
        .setInputFiles(path.join(__dirname, "../fixtures/sample-contract.pdf"))

    const submitBtn = dialog.getByRole("button", { name: "上传并归档" })
    console.log("STEP1 after file+no: disabled =", await submitBtn.isDisabled())

    // 2) 结算主体（同测试 pickOption）
    const input = dialog.getByLabel("结算主体").last()
    await input.click()
    await input.fill(CUSTOMER_NAME)
    const opt = page.getByRole("option").filter({ hasText: CUSTOMER_NAME }).first()
    await opt.waitFor({ timeout: 20_000 })
    console.log("option visible, text:", (await opt.textContent())?.trim())
    await opt.click()
    await page.waitForTimeout(1500)
    console.log("STEP2 after settlement party: disabled =", await submitBtn.isDisabled())
    console.log(
        "settlement hidden value:",
        await dialog.locator('input[value="d565f6a641154b72b18f0d5fa192b09e"]').count(),
    )

    // 3) 显式选择客户（flow-01 的方式，触发 onItemChange -> customerName）
    const custInput = dialog.getByPlaceholder("搜索客户编号或名称")
    await custInput.click()
    await custInput.fill(CUSTOMER_NAME)
    const custOpt = page
        .getByRole("option")
        .filter({ hasText: CUSTOMER_NAME })
        .first()
    await custOpt.waitFor({ timeout: 20_000 })
    await custOpt.click()
    await page.waitForTimeout(1500)
    console.log("STEP3 after explicit customer pick: disabled =", await submitBtn.isDisabled())

    // 4) 若仍禁用：touch 所有可见字段，暴露错误文本
    if (await submitBtn.isDisabled()) {
        for (const label of ["合同编号", "客户", "结算主体", "付款条件"]) {
            const field = dialog.getByLabel(label).last()
            await field.click().catch(() => {})
            await field.press("Tab").catch(() => {})
        }
        await page.waitForTimeout(500)
        console.log("--- invalid fields:", await dialog.locator('[data-invalid="true"]').count())
        const errs = await dialog
            .locator('[role="alert"], [data-slot="field-error"], p')
            .allTextContents()
        console.log("--- error texts:", JSON.stringify(errs.filter((t) => t.trim()).slice(0, 20)))
        console.log("--- dialog text tail:\n", (await dialog.innerText()).slice(-600))
    }

    await browser.close()
}

main().catch((e) => {
    console.error("PROBE FAILED:", e)
    process.exit(1)
})
