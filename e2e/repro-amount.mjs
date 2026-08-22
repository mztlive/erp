import { chromium } from "@playwright/test"

const browser = await chromium.launch()
const context = await browser.newContext()
const page = await context.newPage()
page.on("console", (m) => { if (m.type() === "error") console.log("CONSOLE-ERR:", m.text().slice(0, 120)) })

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
const bankInput = page.getByLabel("银行流水引用").first()
const rec = page.locator("dd", { hasText: /^¥/ }).first()

// Experiment A: fill amount only, read immediately
await amountInput.fill("200.00")
await page.waitForTimeout(200)
console.log("A: amount input value:", await amountInput.inputValue())
console.log("A: 记录金额:", await rec.innerText())

// Experiment B: fill bank ref then amount
await bankInput.fill("E2E-REPRO-3")
await page.waitForTimeout(200)
console.log("B: bank value:", await bankInput.inputValue())
await amountInput.fill("200.00")
await page.waitForTimeout(200)
console.log("B: amount input value:", await amountInput.inputValue())
console.log("B: 记录金额:", await rec.innerText())

// Experiment C: type() instead of fill
await amountInput.fill("")
await amountInput.type("200.00")
await page.waitForTimeout(200)
console.log("C: amount input value:", await amountInput.inputValue())
console.log("C: 记录金额:", await rec.innerText())

// Experiment D: check checkbox AFTER filling amount
await page.getByLabel(/选择 PO-/).first().check()
await page.waitForTimeout(300)
console.log("D: amount input value:", await amountInput.inputValue())
console.log("D: 记录金额:", await rec.innerText())
console.log("D: 本次分配:", await page.getByLabel("本次分配").first().inputValue().catch(() => "N/A"))
console.log("D: disabled:", await page.getByRole("button", { name: "确认登记并核销" }).first().isDisabled())

await browser.close()
