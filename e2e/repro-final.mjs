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

// attach event listeners to the amount input
await page.evaluate(() => {
    window.__events = []
    const el = document.getElementById("amount")
    for (const ev of ["input", "change", "beforeinput"]) {
        el.addEventListener(ev, (e) => {
            window.__events.push(`${ev}:${el.value}@${performance.now().toFixed(0)}`)
        }, true)
    }
})

const ws = page.locator('section[aria-label="供应商核销工作区"]')
const dl = ws.locator("dl").last()
async function snap(label) {
    const props = await page.evaluate(() => {
        const el = document.getElementById("amount")
        const key = Object.keys(el).find((k) => k.startsWith("__reactProps"))
        return el[key].value
    })
    const rec = (await dl.locator("div").first().innerText()).replace(/\n/g, "=")
    const evs = await page.evaluate(() => window.__events.splice(0))
    console.log(`${label}: props=${props} ${rec} events=${evs.join(",")}`)
}

await snap("before fill")
await page.getByLabel("付款金额（含税）").fill("200.00")
await page.waitForTimeout(100)
await snap("t+100")
await page.waitForTimeout(400)
await snap("t+500")
await browser.close()
