import { chromium } from "@playwright/test"

const browser = await chromium.launch()
const context = await browser.newContext()
const page = await context.newPage()
page.on("console", (m) => { if (m.type() === "error") console.log("CONSOLE-ERR:", m.text().slice(0, 150)) })
page.on("pageerror", (e) => console.log("PAGE-ERR:", String(e).slice(0, 200)))

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
console.log("heading:", await heading.innerText())

const pool = page.locator("#alloc-pool")
const poolText = await pool.innerText()
console.log("pool:", poolText.replace(/\n/g, " | "))

const checkbox = page.getByLabel(/选择 PO-/)
console.log("checkbox count:", await checkbox.count())
await checkbox.first().check()
await page.waitForTimeout(500)
const amt = page.getByLabel("本次分配").first()
console.log("本次分配 value:", await amt.inputValue())

const amountInput = page.getByLabel("付款金额（含税）").first()
await amountInput.fill("200.00")
await page.getByLabel("银行流水引用").fill("E2E-REPRO-1")
await page.waitForTimeout(800)

const btn = page.getByRole("button", { name: "确认登记并核销" }).first()
console.log("button disabled:", await btn.isDisabled())
// read validation summary text
const summary = page.locator("ul").filter({ hasText: "核销目标" }).first()
console.log("validation summary visible:", await summary.isVisible().catch(() => false))
if (await summary.isVisible().catch(() => false)) console.log("summary:", await summary.innerText())
// read any red text
const reds = await page.locator(".text-destructive").allInnerTexts()
console.log("red texts:", reds.slice(0, 5))
await page.screenshot({ path: "/tmp/repro-workspace.png" })
await browser.close()
