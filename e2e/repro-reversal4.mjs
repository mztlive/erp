// TEMP repro 4: walk ancestors of the 确认提交 button
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
  await dlg.getByLabel("原因说明").fill("E2E 冲正复现4")
  await dlg.getByRole("button", { name: "下一步" }).click()
  await page.waitForTimeout(2500)

  const info = await page.evaluate(() => {
    const out = []
    const dialog = document.querySelector('[role="alertdialog"]')
    const btns = dialog ? dialog.querySelectorAll("button") : []
    btns.forEach((b) => {
      out.push("BUTTON: " + JSON.stringify(b.textContent?.trim().slice(0, 15)) + " outerHTML=" + b.outerHTML.slice(0, 400))
      let n = b
      let depth = 0
      while (n && depth < 12) {
        const s = getComputedStyle(n)
        out.push(
          `  ^${depth} <${n.tagName.toLowerCase()}> aria-hidden=${n.getAttribute("aria-hidden")} inert=${n.hasAttribute("inert")} hidden=${n.hasAttribute("hidden")} role=${n.getAttribute("role")} display=${s.display} visibility=${s.visibility} aria-disabled=${n.getAttribute("aria-disabled")}`,
        )
        n = n.parentElement
        depth++
      }
    })
    return out
  })
  console.log(info.join("\n"))
  await browser.close()
}

main().catch((e) => {
  console.error("REPRO FAILED:", e.message)
  process.exit(1)
})
