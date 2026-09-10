import { expect, type Page } from "@playwright/test"

const TIMEOUT = 20_000

/**
 * 销售在销售单票款页提交开票申请。批准后才生成 W01 销项开票任务。
 */
export async function submitSalesInvoiceRequest(
    page: Page,
    input: {
        salesOrderId: string
        amount: string
        taxNumber: string
        title?: string
        content?: string
        reason?: string
    },
): Promise<void> {
    await page.goto(`/sales/orders/${input.salesOrderId}?section=receivable`)
    const create = page.locator("#invoice-request-create")
    await expect(create).toBeEnabled({ timeout: TIMEOUT })
    await create.click()
    await expect(page.getByRole("heading", { name: "申请开票" })).toBeVisible({
        timeout: TIMEOUT,
    })
    await expect(page.getByText(/本单可申请/)).toBeVisible({ timeout: TIMEOUT })

    await page.locator("#invoice-request-amount").fill(input.amount)
    if (input.title) {
        await page.locator("#invoice-request-title").fill(input.title)
    } else {
        await expect(page.locator("#invoice-request-title")).not.toHaveValue("")
    }
    await page.locator("#invoice-request-tax-number").fill(input.taxNumber)
    await page.locator("#invoice-request-content").fill(input.content ?? "商品")
    await page
        .locator("#invoice-request-reason")
        .fill(input.reason ?? "销售单已生效，申请开票")

    const submit = page.locator("#invoice-request-submit")
    await expect(submit).toBeEnabled({ timeout: TIMEOUT })
    const posted = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/sales-invoice-requests/submit"),
        { timeout: 60_000 },
    )
    await submit.click({ force: true })
    const response = await posted
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(
        page
            .locator('[aria-label="开票申请详情"]')
            .or(page.getByText("正在读取开票申请"))
            .or(page.getByRole("button", { name: "撤回申请" }))
            .first(),
    ).toBeVisible({ timeout: TIMEOUT })
    await expect(page.locator("#invoice-request-form")).toHaveCount(0)
    await expect(page.locator("#invoice-request-submit")).toHaveCount(0)
}
