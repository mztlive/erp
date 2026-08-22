// TEMP repro 7: enumerate getByRole buttons vs DOM buttons
import { chromium } from "@playwright/test"

const BASE = "http://localhost:3000"

async function main() {
  const browser = await chromium.launch({
    headless: process.env.E2E_HEADED !== "1",
    args: ["--start-maximized"],
  })
  const ctx = await browser.newContext({ viewport: null })
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
  await dlg.getByLabel("原因说明").fill("E2E 冲正复现7")
  await dlg.getByRole("button", { name: "下一步" }).click()
  await page.waitForTimeout(2500)

  // 1) all CSS buttons
  const cssBtns = await page.locator("button").evaluateAll((els) =>
    els.map((b) => {
      const chain = []
      let n = b
      while (n && chain.length < 6) {
        chain.push(`${n.tagName.toLowerCase()}${n.getAttribute("aria-hidden") ? "[aria-hidden]" : ""}${n.getAttribute("role") ? `[role=${n.getAttribute("role")}]` : ""}`)
        n = n.parentElement
      }
      return { text: b.textContent?.trim().slice(0, 12), chain: chain.join(">") }
    }),
  )
  console.log("=== CSS buttons ===")
  cssBtns.forEach((b) => console.log(" ", JSON.stringify(b.text), "|", b.chain))

  // 2) getByRole buttons (visible only, default)
  const roleBtns = await page.getByRole("button").evaluateAll((els) => els.map((b) => b.textContent?.trim().slice(0, 12)))
  console.log("=== getByRole buttons ===")
  console.log(roleBtns)

  // 3) getByRole buttons includeHidden
  const roleBtnsHidden = await page
    .getByRole("button", { includeHidden: true })
    .evaluateAll((els) => els.map((b) => b.textContent?.trim().slice(0, 12)))
  console.log("=== getByRole includeHidden buttons ===")
  console.log(roleBtnsHidden)

  // 4) where is the aria-hidden wrapper relative to the dialog?
  const dialogInfo = await page.evaluate(() => {
    const out = []
    document.querySelectorAll('[role="alertdialog"], [role="dialog"]').forEach((d) => {
      const chain = []
      let n = d
      while (n && chain.length < 5) {
        chain.push(`${n.tagName.toLowerCase()}${n.getAttribute("aria-hidden") ? `[aria-hidden=${n.getAttribute("aria-hidden")}]` : ""}${n.getAttribute("role") ? `[role=${n.getAttribute("role")}]` : ""}${n.getAttribute("data-slot") ? `[${n.getAttribute("data-slot")}]` : ""}`)
        n = n.parentElement
      }
      out.push({ role: d.getAttribute("role"), visible: getComputedStyle(d).visibility, chain: chain.join(">") })
    })
    return out
  })
  console.log("=== dialog chains ===")
  dialogInfo.forEach((d) => console.log(" ", JSON.stringify(d)))
  await browser.close()
}

main().catch((e) => {
  console.error("REPRO FAILED:", e.message)
  process.exit(1)
})
