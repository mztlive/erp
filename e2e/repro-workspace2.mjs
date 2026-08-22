import { chromium } from "@playwright/test"

const browser = await chromium.launch({
    headless: process.env.E2E_HEADED !== "1",
    args: ["--start-maximized"],
})
const context = await browser.newContext({ viewport: null })
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

const heading = page.getByRole("heading", { name: /核销 · / })
await heading.waitFor({ timeout: 20000 })

await page.getByLabel(/选择 PO-/).first().check()
await page.waitForTimeout(300)
console.log("本次分配:", await page.getByLabel("本次分配").first().inputValue())
await page.getByLabel("付款金额（含税）").first().fill("200.00")
await page.getByLabel("银行流水引用").first().fill("E2E-REPRO-2")
await page.waitForTimeout(500)

console.log("disabled:", await page.getByRole("button", { name: "确认登记并核销" }).first().isDisabled())
// full text of the workspace section
const ws = page.locator('section[aria-label="供应商核销工作区"]')
console.log("--- workspace text ---")
console.log((await ws.innerText()).slice(0, 2000))
await browser.close()
