import { expect, type Page } from "@playwright/test"

/** 出纳从唯一待付款任务全额付款，等待正式提交落定后再离开。 */
export async function payOnlySupplierTask(page: Page): Promise<void> {
    await page.goto("/workspace?family=finance")
    const task = page.getByRole("list", { name: "待办列表" }).getByRole("button", { name: /供应商付款处理/ })
    await expect(task).toHaveCount(1, { timeout: 20_000 })
    await task.click()
    const amount = page.locator("#supplier-payables-allocation-form-amount")
    await expect(amount).toHaveValue(/^[1-9]\d*(?:\.\d+)?$|^0\.0*[1-9]\d*$/, { timeout: 20_000 })
    await page.locator("#supplier-payables-allocation-form-bank-receipt-input").setInputFiles({
        name: "e2e-bank-receipt.png",
        mimeType: "image/png",
        buffer: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j5xkAAAAASUVORK5CYII=", "base64"),
    })
    await page.locator("#supplier-payables-allocation-form-submit").click()
    const dialog = page.getByRole("alertdialog").filter({ hasText: "确认付款" })
    await expect(dialog).toBeVisible()
    await expect(dialog.getByText("提交审批")).toHaveCount(0)
    const committed = page.waitForResponse(response => response.request().method() === "POST" && response.url().includes("/admin/supplier-payments/commit"), { timeout: 60_000 })
    await dialog.locator("#supplier-payables-payment-submit-confirm-confirm").click()
    const response = await committed
    expect(response.ok(), await response.text()).toBe(true)
    await expect(dialog).toBeHidden({ timeout: 20_000 })
}
