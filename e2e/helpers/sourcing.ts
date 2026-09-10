import { expect, type Page } from "@playwright/test"

import { expectToast, UI_TIMEOUT } from "./ui"

/**
 * 工作台供给方案默认收起。改选源前先展开「调整方案」。
 */
export async function expandSourcingEditor(
    page: Page,
    productName?: string | RegExp,
): Promise<void> {
    const scope = productName
        ? page
              .getByRole("region", {
                  name:
                      typeof productName === "string"
                          ? `${productName}的供给方案`
                          : new RegExp(`${productName.source}.*的供给方案`),
              })
              .first()
        : page
    const adjust = scope.getByRole("button", { name: "调整方案" }).first()
    if (await adjust.isVisible().catch(() => false)) {
        await adjust.click()
    }
}

/**
 * 预览供给分配后直接确认提交。二次「确认供给分配」弹窗已并入预览框。
 */
export async function confirmSupplyAllocation(
    page: Page,
    success: string | RegExp = /已创建 \d+ 张采购单并提交审批|已将缺口拆成|供给分配已完成|本次全部由现有库存满足/,
): Promise<void> {
    const preview = page.getByRole("dialog", { name: "预览供给分配" })
    if (!(await preview.isVisible().catch(() => false))) {
        await page.locator("#procurement-orders-create-preview").click()
        await expect(preview).toBeVisible({ timeout: UI_TIMEOUT })
    }
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/purchase-orders/from-sourcing"),
        { timeout: 60_000 },
    )
    await preview.locator("#procurement-orders-create-preview-confirm").click()
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    await expectToast(page, success)
}
