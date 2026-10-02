import { expect, type Page } from "@playwright/test"

import { optionalStepVisible } from "./ui"

const TIMEOUT = 20_000
const LONG = 40_000

async function resolveReceiptPartyPicker(page: Page, customerName: string) {
    const sessionHeading = page.getByRole("heading", {
        name: "登记回款",
        exact: true,
    })
    const picker = page
        .getByRole("dialog")
        .filter({ hasText: "登记回款 — 选择往来主体" })
    await Promise.race([
        sessionHeading.waitFor({ state: "visible", timeout: LONG }),
        picker.waitFor({ state: "visible", timeout: LONG }),
    ])
    if (await picker.isVisible().catch(() => false)) {
        const input = picker.locator("#customer-receivables-party-picker-input")
        await input.click()
        await input.fill(customerName)
        const option = page
            .getByRole("option", { name: customerName })
            .or(
                page
                    .locator('[data-slot="combobox-item"]')
                    .filter({ hasText: customerName }),
            )
            .first()
        await expect(option).toBeVisible({ timeout: TIMEOUT })
        await option.click()
        await picker
            .locator("#customer-receivables-party-picker-confirm")
            .click()
    }
    await expect(sessionHeading).toBeVisible({ timeout: LONG })
}

/**
 * 出纳在客户往来页登记一笔回款并核销指定销售单。销售单票款页不再提供登记入口。
 */
export async function registerCustomerReceiptForOrder(
    page: Page,
    input: {
        customerName: string
        orderNo: string
        amount?: string
        bankReference: string
    },
): Promise<{ receiptNo: string }> {
    await page.goto("/finance/customer-accounts")
    await expect(page.getByRole("heading", { name: "客户往来" })).toBeVisible({
        timeout: LONG,
    })
    const register = page.locator(
        "#customer-receivables-header-register-receipt",
    )
    await expect(register).toBeEnabled({ timeout: LONG })
    await register.click()
    await resolveReceiptPartyPicker(page, input.customerName)

    const amountInput = page.locator("#customer-receivables-session-amount")
    await expect(amountInput).toBeVisible({ timeout: TIMEOUT })

    const poolItem = page
        .locator("#customer-receivables-session-allocations")
        .getByRole("row")
        .filter({ hasText: input.orderNo })
    await expect(poolItem).toBeVisible({ timeout: TIMEOUT })
    if (input.amount) {
        await amountInput.fill(input.amount)
    } else if (!(await amountInput.inputValue())) {
        const entryAmount = await poolItem.getByRole("cell").nth(2).innerText()
        const matched = entryAmount.replace(/,/g, "").match(/\d+(?:\.\d+)?/)
        await amountInput.fill(matched?.[0] ?? "")
    }
    const selection = poolItem.getByRole("checkbox")
    if (!(await selection.isChecked())) await selection.check()
    await expect(selection).toBeChecked()
    const fillLine = poolItem.getByRole("button", { name: "填入剩余" })
    await expect(fillLine).toBeVisible({ timeout: TIMEOUT })
    await fillLine.click()
    await page
        .locator("#customer-receivables-session-bank-reference")
        .fill(input.bankReference)
    await page.locator("#customer-receivables-session-submit").click()
    await expect(
        page.getByRole("heading", { name: /提交回款|确认提交回款/ }),
    ).toBeVisible({
        timeout: TIMEOUT,
    })

    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/customer-receipts/commit"),
        { timeout: 60_000 },
    )
    await page
        .locator("#customer-receivables-session-receipt-confirm-dialog-confirm")
        .click()
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    const body = (await response.json()) as {
        data?: { receipt_no?: string; status?: string }
    }
    const receiptNo = body.data?.receipt_no?.trim() ?? ""
    expect(receiptNo.length).toBeGreaterThan(2)
    return { receiptNo }
}

/** 回款预览标题是「回款单：SK-…」，不是单独的单号 heading。 */
export async function expectReceiptPreview(page: Page, receiptNo: string): Promise<void> {
    await expect(
        page.getByText(new RegExp(`回款单：${receiptNo}|${receiptNo}`)).first(),
    ).toBeVisible({ timeout: LONG })
}

/**
 * 客户退款请求弹窗标题是「客户退款」。prepareRefundDraft 会直接 commit，
 * 确认层 AlertDialog 可能不再出现。
 */
export async function submitCustomerRefundRequest(
    page: Page,
    reason: string,
): Promise<{ refundId: string; refundNo: string; status: string }> {
    await expect(
        page.getByRole("dialog").getByRole("heading", { name: /客户退款/ }),
    ).toBeVisible({ timeout: TIMEOUT })
    const reasonInput = page
        .locator("#customer-receivables-refund-request-reason")
        .or(page.locator("#customer-receivables-refund-reason"))
        .first()
    await reasonInput.fill(reason)
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/customer-refunds/commit"),
        { timeout: 60_000 },
    )
    await page.locator("#customer-receivables-refund-request-submit").click()
    const confirm = page.getByRole("alertdialog", { name: /确认提交退款|提交退款/ })
    // 确认框与提交响应谁先到走哪条；prepareRefundDraft 直接提交时不弹确认框。
    if (await optionalStepVisible(confirm, committed)) {
        await page
            .locator("#customer-receivables-refund-submit-confirm-dialog-confirm")
            .click({ force: true })
    }
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    const body = (await response.json()) as {
        data?: {
            id?: string
            refund_id?: string
            refundId?: string
            refund_no?: string
            refundNo?: string
            status?: string
        }
    }
    const data = body.data ?? {}
    const refundId = (data.id ?? data.refund_id ?? data.refundId ?? "").trim()
    const refundNo = (data.refund_no ?? data.refundNo ?? "").trim()
    await expect(page.getByText("退款已提交审批").first()).toBeVisible({
        timeout: LONG,
    })
    return {
        refundId,
        refundNo,
        status: data.status ?? "",
    }
}

/**
 * 回款冲正请求弹窗标题是「回款冲正」。prepareReversalDraft 会直接 commit。
 */
export async function submitReceiptReversalRequest(
    page: Page,
    reason: string,
): Promise<void> {
    await expect(
        page.getByRole("dialog").getByRole("heading", { name: /回款冲正/ }),
    ).toBeVisible({ timeout: TIMEOUT })
    const reasonInput = page
        .locator("#customer-receivables-reversal-request-reason")
        .or(page.locator("#customer-receivables-reversal-reason"))
        .first()
    await reasonInput.fill(reason)
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/receipt-reversals/commit"),
        { timeout: 60_000 },
    )
    await page.locator("#customer-receivables-reversal-request-submit").click()
    const confirm = page.getByRole("alertdialog", { name: /确认提交冲正|提交冲正/ })
    // 确认框与提交响应谁先到走哪条；prepareReversalDraft 直接提交时不弹确认框。
    if (await optionalStepVisible(confirm, committed)) {
        await page
            .locator("#customer-receivables-reversal-submit-confirm-dialog-confirm")
            .click({ force: true })
    }
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(page.getByText("冲正已提交审批").first()).toBeVisible({
        timeout: LONG,
    })
}
