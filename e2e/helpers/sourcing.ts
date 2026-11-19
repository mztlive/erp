import { workPage } from "./workspace-page"
import { expect, type Page } from "@playwright/test"

import { expectToast, UI_TIMEOUT, workspaceTaskId } from "./ui"

/** Open the current product's source adjustment dialog. */
export async function expandSourcingEditor(page: Page, productName?: string | RegExp) {
    const pane = workPage(page)
    const source = typeof productName === "string"
        ? productName.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")
        : productName?.source ?? ".+"
    const adjust = pane.getByRole("region", { name: "供给商品列表", exact: true })
        .getByRole("button", { name: new RegExp(`^调整 (?:${source})`) })
    await expect(adjust).toHaveCount(1, { timeout: UI_TIMEOUT })
    await adjust.click()
    const dialog = pane.getByRole("dialog", { name: new RegExp(`^供给分配 · (?:${source})`) })
    await expect(dialog).toBeVisible({ timeout: UI_TIMEOUT })
    return dialog
}

/**
 * 预览供给分配后直接确认提交。二次「确认供给分配」弹窗已并入预览框。
 */
export async function confirmSupplyAllocation(
    page: Page,
    success:
        | string
        | RegExp = /已创建 \d+ 张采购单并提交审批|已将缺口拆成|供给分配已完成|本次全部由现有库存满足/,
) {
    const preview = workPage(page).getByRole("dialog", {
        name: "预览供给分配",
        exact: true,
    })
    if (!(await preview.isVisible())) {
        await workPage(page).locator("#procurement-orders-create-preview").click()
    }
    await expect(preview).toBeVisible({ timeout: UI_TIMEOUT })
    const taskId = workspaceTaskId(page)
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/purchase-orders/from-sourcing"),
        { timeout: 60_000 },
    ).then(async (response) => ({ response, body: await response.text() }))
    await preview.locator("#procurement-orders-create-preview-confirm").click()
    const { response, body } = await committed
    const failure = `供给分配提交失败（HTTP ${response.status()}）: ${body}`
    expect(response.ok(), failure).toBeTruthy()
    const result = JSON.parse(body)
    expect(result.success, failure).toBe(true)
    if (taskId && result.data?.work_item_status === "COMPLETED") {
        // Task completion replaces the detail tab and its transient toast.
        await expect.poll(() => workspaceTaskId(page), { timeout: UI_TIMEOUT }).not.toBe(taskId)
    } else {
        await expectToast(page, success)
    }
    return result.data as {
        orders: Array<{ purchase_order_id: string; purchase_no: string }>
        stock_reservations: Array<{ sales_order_line_id: string }>
        work_item_status: string
    }
}
