/* eslint-disable no-console */
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
    await page.waitForTimeout(2500)
    const hits = await page.evaluate(async () => {
        const out: string[] = []
        for (const s of Array.from(document.querySelectorAll("script[src]"))) {
            const src = s.getAttribute("src")!
            try {
                const text = await (await fetch(src)).text()
                if (text.includes("matchMedia") && text.includes("narrowDetailOpen")) {
                    out.push("matchMedia-fix:" + text.includes("1023.98px"))
                }
                if (text.includes("决策成功后刷新待办列表")) {
                    out.push("refetch-fix:" + (text.match(/决策成功后刷新待办列表/g) ?? []).length)
                }
            } catch { /* ignore */ }
        }
        return out
    })
    console.log(JSON.stringify(hits))
    await browser.close()
}
main().catch((e) => { console.error("PROBE FAILED:", e); process.exit(1) })
