import { workPage } from "./workspace-page"
import { expect, type Page } from "./test"
import { chooseOption } from "./ui"

export async function setBeijingShippingAddress(page: Page): Promise<void> {
    await workPage(page).locator("#sales-orders-create-unified-address-open").click()
    const dialog = workPage(page).getByRole("dialog", { name: "设置统一收货地址", exact: true })
    await dialog.getByLabel("收货人").fill("李测")
    await dialog.getByLabel("联系电话").fill("13800138001")
    for (const [level, name] of [["province", "北京市"], ["city", "市辖区"], ["district", "朝阳区"]]) {
        await chooseOption(page, dialog.locator(`#sales-orders-create-unified-address-region-${level}`), name!)
    }
    await dialog.getByLabel("详细地址").fill("测试路 1 号")
    await dialog.getByRole("button", { name: "应用到全部商品", exact: true }).click()
    await expect(dialog).toBeHidden()
}

/** 从开单入口选择归档合同，确认后等待合同摘要带入表单。 */
export async function selectExistingContractForSalesOrder(
    page: Page,
    contractNo: string,
): Promise<void> {
    await workPage(page).locator("#sales-orders-create-entry-contract").click()
    const dialog = workPage(page).getByRole("dialog", { name: "选择已有合同" })
    await expect(dialog).toBeVisible()
    const input = dialog.locator("#sales-contract-picker-contract")
    // 先等远程搜索返回，避免在防抖查询替换旧候选时点击被卸载的选项。
    const searched = page.waitForResponse((response) => {
        const url = new URL(response.url())
        return response.request().method() === "GET" &&
            url.pathname === "/admin/contracts" && url.searchParams.get("q") === contractNo
    })
    await input.fill(contractNo)
    const response = await searched
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(input).not.toHaveAttribute("aria-busy", "true")
    await chooseOption(
        page,
        input,
        contractNo,
        contractNo,
    )
    await dialog.locator("#sales-contract-picker-confirm").click()
    await expect(dialog).toBeHidden()
    await expect(workPage(page).locator("#contractId")).toContainText(contractNo)
    await expect(workPage(page).getByRole("heading", { name: "合同信息", exact: true })).toBeVisible()
}

/** 合同带入的客户与付款条件必须以只读摘要显示。 */
export async function expectSalesOrderContractSummary(
    page: Page,
    input: { customerName?: string; paymentTerms?: string; contractNo?: string },
): Promise<void> {
    if (input.customerName) {
        const customer = workPage(page).locator("#sales-orders-create-customer")
        await expect(customer).toContainText(input.customerName)
        await expect(customer.locator("input, button, [role=combobox]")).toHaveCount(0)
    }
    if (input.paymentTerms) {
        const payment = workPage(page).locator("#sales-orders-create-header-payment-terms")
        await expect(payment).toContainText(input.paymentTerms)
        await expect(payment.locator("input, button, [role=combobox]")).toHaveCount(0)
    }
    if (input.contractNo) {
        await expect(workPage(page).locator("#contractId")).toContainText(`${input.contractNo} · v1`)
    }
}

/** 成交单价由销售填写：选择商品或修改数量都不带入参考价（第一期 §4.4.2）。 */
export async function fillSalesLineUnitPrice(
    page: Page,
    unitPrice: string,
    lineIndex = 0,
): Promise<void> {
    const input = workPage(page)
        .locator('input[id^="sales-orders-create-line-"][id$="-unit-price"]')
        .nth(lineIndex)
    await expect(input).toBeVisible()
    await input.fill(unitPrice)
}
