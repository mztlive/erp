import { chromium } from "@playwright/test"

const browser = await chromium.launch()
const context = await browser.newContext()
const page = await context.newPage()

await page.goto("http://localhost:3000/login")
await page.getByPlaceholder("请输入账号").fill("caiwu")
await page.getByPlaceholder("请输入密码").fill("123456")
await page.getByRole("button", { name: /登录/ }).click()
await page.waitForURL(/workspace/, { timeout: 20000 })

await page.goto("http://localhost:3000/finance/supplier-accounts")
await page.getByRole("button", { name: "登记付款" }).first().click()
const dlg = page.locator('[role="dialog"]:visible, [role="alertdialog"]:visible').last()
await dlg.getByPlaceholder("选择供应商").click()
await dlg.getByPlaceholder("选择供应商").fill("主太帅")
await page.locator('[role="listbox"]').last().getByText("主太帅", { exact: false }).first().waitFor({ timeout: 20000 })
await page.locator('[role="listbox"]').last().getByText("主太帅", { exact: false }).first().click()
await dlg.getByRole("button", { name: "进入本次核销" }).click()
await page.getByRole("heading", { name: /核销 · / }).waitFor({ timeout: 20000 })

async function readStats(label) {
    const ws = page.locator('section[aria-label="供应商核销工作区"]')
    const dl = ws.locator("dl").last()
    const rows = dl.locator("div")
    const out = []
    for (let i = 0; i < await rows.count(); i++) {
        const t = (await rows.nth(i).innerText()).replace(/\n/g, "=")
        out.push(t)
    }
    console.log(label, "->", out.join(" | "))
    const summary = ws.locator('[data-slot="validation-summary"]')
    console.log(label, "summary:", (await summary.isVisible().catch(() => false)) ? (await summary.innerText()).replace(/\n/g, " | ") : "NONE")
    console.log(label, "disabled:", await page.getByRole("button", { name: "确认登记并核销" }).first().isDisabled())
}

const amountInput = page.getByLabel("付款金额（含税）").first()
await readStats("initial")
await amountInput.fill("200.00")
await page.waitForTimeout(300)
await readStats("after amount fill")
await page.getByLabel(/选择 PO-/).first().check()
await page.waitForTimeout(300)
await readStats("after check")
await page.getByLabel("银行流水引用").first().fill("E2E-REPRO-4")
await page.waitForTimeout(300)
await readStats("after bank fill")
await browser.close()
