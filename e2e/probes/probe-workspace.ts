/* eslint-disable no-console */
// 探测：工作台点击任务后 section[aria-label="当前任务"] 的数量与内容
import { chromium } from "@playwright/test"
import { loginViaUi } from "../helpers/login"

async function main() {
    const browser = await chromium.launch({
        headless: process.env.E2E_HEADED !== "1",
        args: ["--start-maximized"],
    })
    const context = await browser.newContext({
        baseURL: "http://localhost:3000",
        viewport: null,
    })
    const page = await context.newPage()
    await loginViaUi(page, "procurement")
    await page.goto("/workspace")
    const task = page.getByRole("list", { name: "待办列表" }).getByRole("button").first()
    await task.waitFor({ timeout: 30_000 })
    console.log("task found, clicking")
    await task.click()
    await page.waitForTimeout(3000)
    const sections = page.locator('section[aria-label="当前任务"]')
    console.log("section count:", await sections.count())
    for (let i = 0; i < (await sections.count()); i++) {
        const s = sections.nth(i)
        const visible = await s.isVisible().catch(() => false)
        const btns = await s.getByRole("button").allTextContents()
        console.log(`section[${i}] visible=${visible} buttons=${JSON.stringify(btns.map((b) => b.trim().slice(0, 20)))}`)
        console.log("  text:", (await s.innerText().catch(() => ""))?.slice(0, 120).replace(/\n/g, " | "))
    }
    // 看 dialog 数量
    console.log("dialogs:", await page.getByRole("dialog").count())
    await browser.close()
}
main().catch((e) => {
    console.error("PROBE FAILED:", e)
    process.exit(1)
})
