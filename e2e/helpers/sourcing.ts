import { expect, type Page } from "@playwright/test"

import { expectToast, UI_TIMEOUT } from "./ui"

/** 在供给商品列表选择要编辑的真实商品，等待独立编辑区切换完成。 */
export async function expandSourcingEditor(
    page: Page,
    productName?: string | RegExp,
): Promise<void> {
    const task = page.getByRole("region", { name: "当前供给分配任务" })
    const editor = task.getByRole("region", { name: "当前商品供给编辑" })
    if (productName) {
        const source = typeof productName === "string"
            ? productName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
            : productName.source
        const adjust = task.getByRole("region", { name: "供给商品列表" })
            .getByRole("button", { name: new RegExp(`^调整[\\s\\S]*(?:${source})`) })
        await expect(adjust).toBeVisible({ timeout: UI_TIMEOUT })
        await adjust.click()
        await expect(editor.getByRole("heading", { name: productName })).toBeVisible({ timeout: UI_TIMEOUT })
    }
    await expect(editor).toBeVisible({ timeout: UI_TIMEOUT })
}

/**
 * 预览供给分配后直接确认提交。二次「确认供给分配」弹窗已并入预览框。
 */
export async function confirmSupplyAllocation(
    page: Page,
    success:
        | string
        | RegExp = /已创建 \d+ 张采购单并提交审批|已将缺口拆成|供给分配已完成|本次全部由现有库存满足/,
): Promise<void> {
    const preview = page.getByRole("dialog", {
        name: "预览供给分配",
        exact: true,
    })
    if (!(await preview.isVisible())) {
        await page.locator("#procurement-orders-create-preview").click()
    }
    await expect(preview).toBeVisible({ timeout: UI_TIMEOUT })
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/purchase-orders/from-sourcing"),
        { timeout: 60_000 },
    )
    await preview.locator("#procurement-orders-create-preview-confirm").click()
    const response = await committed
    const body = await response.text()
    const failure = `供给分配提交失败（HTTP ${response.status()}）: ${body}`
    expect(response.ok(), failure).toBeTruthy()
    expect(JSON.parse(body).success, failure).toBe(true)
    await expectToast(page, success)
}
