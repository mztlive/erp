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
await page.getByRole("heading", { name: /核销 · / }).waitFor({ timeout: 20000 })
await page.waitForTimeout(1000)

const ws = page.locator('section[aria-label="供应商核销工作区"]')
async function rec() {
    return (await ws.locator("dl").last().locator("div").first().innerText()).replace(/\n/g, "=")
}

// 1) native setter + input event dispatch
await page.evaluate(() => {
    const el = document.getElementById("amount")
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set
    setter.call(el, "200.00")
    el.dispatchEvent(new Event("input", { bubbles: true }))
})
await page.waitForTimeout(300)
console.log("after native dispatch:", await page.locator("#amount").inputValue(), "|", await rec())

// 2) direct onChange prop invocation
await page.evaluate(() => {
    const el = document.getElementById("amount")
    const key = Object.keys(el).find((k) => k.startsWith("__reactProps"))
    const props = el[key]
    props.onChange({ target: { value: "300.00" } })
})
await page.waitForTimeout(300)
console.log("after direct onChange:", await page.locator("#amount").inputValue(), "|", await rec())

// 3) keyboard typing
await page.locator("#amount").fill("")
await page.locator("#amount").pressSequentially("400.00", { delay: 30 })
await page.waitForTimeout(300)
console.log("after pressSequentially:", await page.locator("#amount").inputValue(), "|", await rec())
await browser.close()
