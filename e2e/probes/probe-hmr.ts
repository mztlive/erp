/* eslint-disable no-console */
// 探测：确认前端 dev server 是否已编译含"决策后刷新列表"修复的 workspace chunk
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
    await page.waitForTimeout(3000)
    const chunkUrls = await page.evaluate(async () => {
        const scripts = Array.from(
            document.querySelectorAll("script[src]"),
        ).map((s) => s.getAttribute("src")!)
        const found: string[] = []
        for (const src of scripts) {
            try {
                const res = await fetch(src)
                const text = await res.text()
                if (text.includes("selectNextAfter")) {
                    found.push(src)
                }
            } catch {
                // 忽略跨域/加载失败
            }
        }
        return found
    })
    console.log("chunks with selectNextAfter:", chunkUrls.length)
    for (const u of chunkUrls) {
        const res = await context.request.get(`http://localhost:3000${u}`)
        const text = await res.text()
        console.log(
            u.slice(0, 120),
            "| hasFix:",
            text.includes("决策成功后刷新待办列表"),
            "| hasRefetch:",
            text.includes("void dashboardQuery.refetch()"),
        )
    }
    await browser.close()
}
main().catch((e) => {
    console.error("PROBE FAILED:", e)
    process.exit(1)
})
