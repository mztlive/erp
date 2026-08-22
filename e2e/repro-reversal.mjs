// TEMP repro: observe the payment-reversal UX after 下一步
import { chromium } from "@playwright/test"

const BASE = "http://localhost:3000"
const API = "http://127.0.0.1:10001"

async function main() {
  const browser = await chromium.launch()
  const ctx = await browser.newContext()
  const page = await ctx.newPage()
  page.on("console", (m) => {
    if (m.type() === "error") console.log("[console.error]", m.text().slice(0, 200))
  })

  // login as caiwu
  await page.goto(`${BASE}/login`)
  await page.getByLabel(/账号|用户名/).first().fill("caiwu")
  await page.getByLabel(/密码/).first().fill("123456")
  await page.getByRole("button", { name: "登录" }).first().click()
  await page.waitForURL(/workspace|finance/, { timeout: 20000 }).catch(() => {})

  await page.goto(`${BASE}/finance/supplier-accounts?view=payment`)
  await page.waitForTimeout(2500)

  // find a posted payment row with a 冲正 button
  const row = page.locator("table").first().getByRole("row").filter({ hasText: "已过账" }).first()
  await row.waitFor({ state: "visible", timeout: 20000 })
  console.log("ROW TEXT:", (await row.innerText()).slice(0, 300).replace(/\n/g, " | "))

  const reverseBtn = row.getByRole("button", { name: "冲正", exact: true }).first()
  await reverseBtn.click()
  const dlg = page.locator('[role="dialog"]:visible, [role="alertdialog"]:visible').last()
  await dlg.waitFor({ state: "visible", timeout: 10000 })
  console.log("DIALOG TEXT:", (await dlg.innerText().catch(() => ""))?.slice(0, 300).replace(/\n/g, " | "))

  await dlg.getByLabel("原因说明").fill("E2E 冲正复现")
  await dlg.getByRole("button", { name: "下一步" }).click()

  await page.waitForTimeout(3000)
  console.log("URL:", page.url())
  const visible = page.locator('[role="dialog"]:visible, [role="alertdialog"]:visible, [data-slot="sheet-content"]:visible')
  const count = await visible.count()
  console.log("visible dialogs/sheets:", count)
  for (let i = 0; i < count; i++) {
    const t = (await visible.nth(i).innerText().catch(() => "")).slice(0, 500).replace(/\n/g, " | ")
    console.log(`--- overlay ${i}:`, t)
  }
  const bodyText = (await page.locator("body").innerText()).slice(0, 1200).replace(/\n/g, " | ")
  console.log("BODY TEXT:", bodyText)

  await browser.close()
}

main().catch((e) => {
  console.error("REPRO FAILED:", e.message)
  process.exit(1)
})
