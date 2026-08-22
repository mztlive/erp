import { chromium } from "@playwright/test"

const browser = await chromium.launch({
    headless: process.env.E2E_HEADED !== "1",
    args: ["--start-maximized"],
})
const context = await browser.newContext({ viewport: null })
const page = await context.newPage()
page.on("pageerror", (e) => console.log("PAGE-ERR:", String(e).slice(0, 250)))
page.on("console", (m) => { if (m.type() === "error") console.log("CONSOLE-ERR:", m.text().slice(0, 150)) })

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
await page.waitForTimeout(800)

async function propsOf(id) {
    return page.evaluate((id) => {
        const el = document.getElementById(id)
        const key = Object.keys(el).find((k) => k.startsWith("__reactProps"))
        const props = el[key]
        return { value: props.value, onChangeSrc: props.onChange?.toString().slice(0, 90) }
    }, id)
}

console.log("amount before:", JSON.stringify(await propsOf("amount")))
console.log("bank before:", JSON.stringify(await propsOf("bankReference")))

await page.getByLabel("付款金额（含税）").fill("200.00")
await page.getByLabel("银行流水引用").fill("E2E-BANK-1")
await page.waitForTimeout(400)

console.log("amount after:", JSON.stringify(await propsOf("amount")))
console.log("bank after:", JSON.stringify(await propsOf("bankReference")))
await browser.close()
