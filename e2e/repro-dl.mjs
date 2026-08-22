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
await page.waitForTimeout(500)

const ws = page.locator('section[aria-label="供应商核销工作区"]')
const dls = ws.locator("dl")
console.log("dl count:", await dls.count())
for (let i = 0; i < await dls.count(); i++) {
    console.log(`--- dl[${i}] ---`)
    console.log((await dls.nth(i).innerText()).replace(/\n/g, " | "))
}

// now fill and re-read the FORM's own summary
await page.getByLabel("付款金额（含税）").fill("200.00")
await page.waitForTimeout(300)
console.log("=== after fill ===")
for (let i = 0; i < await dls.count(); i++) {
    console.log(`--- dl[${i}] ---`)
    console.log((await dls.nth(i).innerText()).replace(/\n/g, " | "))
}
await browser.close()
