import { chromium } from "@playwright/test"

const browser = await chromium.launch()
const context = await browser.newContext()
const page = await context.newPage()
page.on("console", (m) => {
    const t = m.text()
    if (t.includes("[alloc-debug]")) console.log("DBG:", t.slice(0, 300))
})

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

const ws = page.locator('section[aria-label="供应商核销工作区"]')
async function rec() {
    return (await ws.locator("dl").last().locator("div").first().innerText()).replace(/\n/g, "=")
}

console.log("== fill amount ==")
await page.getByLabel("付款金额（含税）").fill("200.00")
await page.waitForTimeout(400)
console.log("记录金额:", await rec())

console.log("== fill bank ==")
await page.getByLabel("银行流水引用").fill("E2E-DEBUG-1")
await page.waitForTimeout(400)
console.log("记录金额:", await rec())

console.log("== check po ==")
await page.getByLabel(/选择 PO-/).first().check()
await page.waitForTimeout(600)
console.log("记录金额:", await rec())
console.log("disabled:", await page.getByRole("button", { name: "确认登记并核销" }).first().isDisabled())
await browser.close()
