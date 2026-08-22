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

const amountInput = page.getByLabel("付款金额（含税）").first()
console.log("outerHTML:", (await amountInput.evaluate((el) => el.outerHTML)).slice(0, 400))
console.log("defaultValue attr:", await amountInput.getAttribute("value"))

// React onInput listener presence
const hasReactListener = await amountInput.evaluate((el) => {
    const key = Object.keys(el).find((k) => k.startsWith("__reactProps"))
    if (!key) return "no react props"
    const props = el[key]
    return `onChange:${typeof props.onChange} onInput:${typeof props.onInput} value:${props.value}`
})
console.log("react props:", hasReactListener)

// fill and watch every 100ms
await amountInput.fill("200.00")
for (let i = 0; i < 5; i++) {
    await page.waitForTimeout(100)
    const v = await amountInput.inputValue()
    const ws = page.locator('section[aria-label="供应商核销工作区"]')
    const rec = await ws.locator("dl").last().locator("div").first().innerText()
    console.log(`t+${(i + 1) * 100}ms input=${v} 记录金额=${rec.replace(/\n/g, "=")}`)
}
await browser.close()
