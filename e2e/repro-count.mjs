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

// count all inputs with label 付款金额
const labeled = page.getByLabel("付款金额（含税）")
console.log("labeled count:", await labeled.count())
for (let i = 0; i < await labeled.count(); i++) {
    const el = labeled.nth(i)
    console.log("  input", i, "id:", await el.getAttribute("id"), "name:", await el.getAttribute("name"), "value:", await el.inputValue(), "readonly:", await el.getAttribute("readonly"), "disabled:", await el.isDisabled())
}
// count ALL inputs in the page with id=amount
const idAmount = page.locator('input[id="amount"]')
console.log("id=amount count:", await idAmount.count())
for (let i = 0; i < await idAmount.count(); i++) {
    const el = idAmount.nth(i)
    console.log("  input", i, "value:", await el.inputValue(), "visible:", await el.isVisible().catch(() => false))
}
// fill the FIRST labeled input and check form behavior via the second
if ((await labeled.count()) > 1) {
    await labeled.nth(1).fill("200.00")
    await page.waitForTimeout(300)
    console.log("after fill nth(1): input0:", await labeled.nth(0).inputValue(), "input1:", await labeled.nth(1).inputValue())
    const ws = page.locator('section[aria-label="供应商核销工作区"]')
    const dl = ws.locator("dl").last()
    console.log("记录金额 now:", (await dl.locator("div").first().innerText()).replace(/\n/g, "="))
}
await browser.close()
