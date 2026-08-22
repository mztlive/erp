// TEMP repro 3: inspect the confirm button DOM/accessibility state
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
  await dlg.getByLabel("原因说明").fill("E2E 冲正复现3")
  await dlg.getByRole("button", { name: "下一步" }).click()
  await page.waitForTimeout(2500)

  const info = await page.evaluate(() => {
    const out = []
    const dialogs = document.querySelectorAll('[role="dialog"], [role="alertdialog"]')
    out.push("dialogs: " + dialogs.length)
    dialogs.forEach((d, i) => {
      const r = d.getAttribute("role")
      const vis = getComputedStyle(d).visibility
      const disp = getComputedStyle(d).display
      out.push(`[${i}] role=${r} visible=${vis} display=${disp} inert=${d.hasAttribute("inert")}`)
      const btns = d.querySelectorAll("button")
      btns.forEach((b, j) => {
        const style = getComputedStyle(b)
        out.push(
          `  btn[${j}] text=${JSON.stringify(b.textContent?.trim().slice(0, 20))} aria-label=${JSON.stringify(b.getAttribute("aria-label"))} disabled=${b.disabled} aria-disabled=${b.getAttribute("aria-disabled")} vis=${style.visibility} disp=${style.display} pointerEvents=${style.pointerEvents} name=${JSON.stringify((b.getAttribute("aria-label") || b.textContent || "").trim().slice(0, 20))}`,
        )
      })
    })
    return out
  })
  console.log(info.join("\n"))

  // try alternate queries
  const byText = page.getByText("确认提交", { exact: true })
  console.log("getByText exact count:", await byText.count())
  const byRoleRegex = page.getByRole("button", { name: /确认提交/ })
  console.log("getByRole regex count:", await byRoleRegex.count())
  const allButtons = page.getByRole("button")
  console.log("all buttons count:", await allButtons.count())

  await browser.close()
}

main().catch((e) => {
  console.error("REPRO FAILED:", e.message)
  process.exit(1)
})
