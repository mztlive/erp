import { expect, test } from "@playwright/test"

import { FRONTEND_BASE_URL } from "./env"
import { workPage } from "./workspace-page"

const origin = new URL(FRONTEND_BASE_URL).origin

test("定位当前活动的业务页签，忽略工作区外壳和保留的非活动文档", async ({ page }) => {
    await page.route(`${origin}/active-document`, (route) => route.fulfill({
        contentType: "text/html; charset=utf-8",
        body: `<h1>待我处理</h1><button onclick="this.textContent='已处理'">处理</button>`,
    }))
    await page.route(`${origin}/workspace/tasks`, (route) => route.fulfill({
        contentType: "text/html; charset=utf-8",
        body: `
            <h1>工作区外壳</h1>
            <iframe data-workspace-active="false" srcdoc="<h1>已缓存详情</h1>"></iframe>
            <iframe data-workspace-active="true" src="/active-document"></iframe>`,
    }))
    await page.goto(`${origin}/workspace/tasks`)
    await expect(workPage(page).getByRole("heading", { name: "待我处理" })).toBeVisible()
    await expect(workPage(page).getByRole("heading", { name: "已缓存详情" })).toHaveCount(0)
    await workPage(page).getByRole("button", { name: "处理", exact: true }).click()
    await expect(workPage(page).getByRole("button", { name: "已处理" })).toBeVisible()
})

test("登录和公开选品页使用页面本身", async ({ page }) => {
    for (const path of ["/login", "/s/public-selection"]) {
        await page.route(`${origin}${path}`, (route) => route.fulfill({
            contentType: "text/html; charset=utf-8", body: `<h1>${path}</h1>`,
        }))
        await page.goto(`${origin}${path}`)
        await expect(workPage(page).getByRole("heading", { name: path })).toBeVisible()
    }
})
