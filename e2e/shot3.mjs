import { chromium } from "@playwright/test"
const OUT = "/private/tmp/claude-501/-Users-huangjiajiang-Development-erp/fda94318-8842-42d7-bbfe-c5956adaaa39/scratchpad"
const browser = await chromium.launch({ headless: true })
const ctx = await browser.newContext({ viewport: { width: 1600, height: 1000 } })
const page = await ctx.newPage()
page.on("pageerror", (e) => console.log("PAGEERROR:", e.message.slice(0,200)))
await page.goto("http://localhost:3000/login")
await page.getByPlaceholder("请输入账号").fill("admin")
await page.getByPlaceholder("请输入密码").fill("123456")
await page.getByRole("button", { name: "登录" }).click()
await page.waitForURL((u) => !u.pathname.includes("/login"), { timeout: 30000 })
await page.waitForTimeout(1500)
async function shot(url, name, after) {
  await page.goto("http://localhost:3000" + url)
  await page.waitForTimeout(4000)
  if (after) { try { await after() } catch (e) { console.log("after fail", name, e.message.slice(0,120)) } }
  await page.screenshot({ path: `${OUT}/${name}.png` })
  console.log("shot", name, "->", page.url())
}
await shot("/system/access-audit?view=roles", "11-roles-new")
await shot("/system/access-audit?view=users", "12-users-new")
await shot("/system/audit", "13-audit-new")
await shot("/system/accounts", "14-accounts-new")
await shot("/system/access-audit?view=roles", "15-explain-sheet", async () => {
  await page.getByRole("row").nth(2).click()
  await page.waitForTimeout(2000)
})
await browser.close()
