// TEMP repro 2: exact test wait semantics for the reversal confirm dialog
import { chromium } from "@playwright/test"

const BASE = "http://localhost:3000"

async function main() {
  const browser = await chromium.launch()
  const ctx = await browser.newContext()
  const page = await ctx.newPage()

  await page.goto(`${BASE}/login`)
  await page.getByLabel(/账号|用户名/).first().fill("caiwu")
  await page.getByLabel(/密码/).first().fill("123456")
  await page.getByRole("button", { name: "登录" }).first().click()
  await page.waitForURL(/workspace|finance/, { timeout: 20000 }).catch(() => {})

  await page.goto(`${BASE}/finance/supplier-accounts?view=payment`)
  await page.waitForTimeout(2500)

  const row = page.locator("table").first().getByRole("row").filter({ hasText: "已过账" }).first()
  await row.waitFor({ state: "visible", timeout: 20000 })
  await row.getByRole("button", { name: "冲正", exact: true }).first().click()

  const dlg = page.locator('[role="dialog"]:visible, [role="alertdialog"]:visible').last()
  await dlg.waitFor({ state: "visible", timeout: 10000 })
  await dlg.getByLabel("原因说明").fill("E2E 冲正复现2")
  await dlg.getByRole("button", { name: "下一步" }).click()

  // exactly like the test: wait up to 20s for the button
  const btn = page
    .locator('[role="alertdialog"]:visible, [role="dialog"]:visible')
    .getByRole("button", { name: "确认提交" })
    .first()
  try {
    await btn.waitFor({ state: "visible", timeout: 20000 })
    console.log("OK: 确认提交 button visible")
  } catch {
    console.log("FAIL: button not visible in 20s")
  }
  // what overlays remain?
  const overlays = page.locator('[role="alertdialog"]:visible, [role="dialog"]:visible')
  const n = await overlays.count()
  console.log("visible overlays now:", n)
  for (let i = 0; i < n; i++) {
    console.log(`--- overlay ${i}:`, (await overlays.nth(i).innerText().catch(() => "")).slice(0, 220).replace(/\n/g, " | "))
  }
  await browser.close()
}

main().catch((e) => {
  console.error("REPRO FAILED:", e.message)
  process.exit(1)
})
