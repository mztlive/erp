// TEMP repro 6: CDP accessibility tree for the confirm dialog buttons
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
  await dlg.getByLabel("原因说明").fill("E2E 冲正复现6")
  await dlg.getByRole("button", { name: "下一步" }).click()
  await page.waitForTimeout(2500)

  const cdp = await ctx.newCDPSession(page)
  const { nodes } = await cdp.send("Accessibility.getFullAXTree")
  const interesting = nodes
    .filter((n) => {
      const name = n.name?.value || ""
      return name.includes("确认提交") || name.includes("返回修改") || name.includes("冲正") || name.includes("提交")
    })
    .map((n) => ({
      role: n.role?.value,
      name: n.name?.value,
      ignored: n.ignored,
      backendDOMNodeId: n.backendDOMNodeId,
    }))
  console.log(JSON.stringify(interesting, null, 1))
  await browser.close()
}

main().catch((e) => {
  console.error("REPRO FAILED:", e.message)
  process.exit(1)
})
