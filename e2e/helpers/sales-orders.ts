import { expect, type Page } from "./test"
import { chooseOption } from "./ui"

/** 从开单入口选择归档合同，确认后等待合同摘要带入表单。 */
export async function selectExistingContractForSalesOrder(
    page: Page,
    contractNo: string,
): Promise<void> {
    await page.locator("#sales-orders-create-entry-contract").click()
    const dialog = page.getByRole("dialog", { name: "选择已有合同" })
    await expect(dialog).toBeVisible()
    await chooseOption(
        page,
        dialog.locator("#sales-contract-picker-contract"),
        contractNo,
        contractNo,
    )
    await dialog.locator("#sales-contract-picker-confirm").click()
    await expect(dialog).toBeHidden()
    await expect(page.locator("#contractId")).toContainText(contractNo)
    await expect(page.getByRole("heading", { name: "合同信息", exact: true })).toBeVisible()
}

/** 合同带入的客户与付款条件必须以只读摘要显示。 */
export async function expectSalesOrderContractSummary(
    page: Page,
    input: { customerName?: string; paymentTerms?: string; contractNo?: string },
): Promise<void> {
    if (input.customerName) {
        const customer = page.locator("#sales-orders-create-customer")
        await expect(customer).toContainText(input.customerName)
        await expect(customer.locator("input, button, [role=combobox]")).toHaveCount(0)
    }
    if (input.paymentTerms) {
        const payment = page.locator("#sales-orders-create-header-payment-terms")
        await expect(payment).toContainText(input.paymentTerms)
        await expect(payment.locator("input, button, [role=combobox]")).toHaveCount(0)
    }
    if (input.contractNo) {
        await expect(page.locator("#contractId")).toContainText(`${input.contractNo} · v1`)
    }
}
