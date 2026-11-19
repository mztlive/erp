import { workPage } from "./workspace-page"
import { expect, type Page } from "@playwright/test"
import { fileURLToPath } from "node:url"

import { optionalStepVisible } from "./ui"

const TIMEOUT = 20_000
const LONG = 40_000

async function resolveReceiptPartyPicker(page: Page, customerName: string) {
    const sessionHeading = workPage(page).getByRole("heading", {
        name: "登记回款",
        exact: true,
    })
    const picker = workPage(page)
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
        const option = workPage(page)
            .getByRole("option", { name: customerName })
            .or(
                workPage(page)
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

export async function startReceiptSession(page: Page, customerName: string) {
    await page.goto("/finance/customer-accounts")
    await expect(workPage(page).getByRole("heading", { name: "客户往来" })).toBeVisible({
        timeout: LONG,
    })
    const register = workPage(page).locator("#customer-receivables-header-register-receipt")
    await expect(register).toBeEnabled({ timeout: LONG })
    await register.click()
    await resolveReceiptPartyPicker(page, customerName)
    await expect(workPage(page).locator("#customer-receivables-session-allocations-title")).toBeVisible()
}

export function receiptAllocationRow(page: Page, orderNo: string) {
    return workPage(page).locator("#customer-receivables-session-allocations")
        .getByRole("row").filter({ hasText: orderNo })
}

export async function selectReceiptAllocations(
    page: Page,
    orderNos: readonly string[],
    verifySearch = false,
) {
    await workPage(page).locator("#customer-receivables-session-add-receivable").click()
    const picker = workPage(page).getByRole("dialog", { name: "添加应收", exact: true })
    const search = picker.locator("#customer-receivables-session-allocation-search")
    for (const orderNo of orderNos) {
        await search.fill(orderNo)
        const target = picker.getByRole("row").filter({ hasText: orderNo })
        await target.getByRole("checkbox").check()
    }
    if (verifySearch) {
        await search.fill("不存在的应收项目")
        await expect(picker.getByText("没有符合搜索条件的应收。")).toBeVisible()
        await expect(picker.getByRole("status")).toHaveText(`已选 ${orderNos.length} 笔（含搜索结果外）`)
        for (const orderNo of orderNos) {
            await expect(receiptAllocationRow(page, orderNo)).toHaveCount(0)
        }
    }
    await picker.locator("#customer-receivables-picker-confirm").click()
    await expect(picker).toBeHidden()
    for (const orderNo of orderNos) {
        await expect(receiptAllocationRow(page, orderNo)).toBeVisible()
    }
}

export async function uploadReceiptEvidence(page: Page) {
    await workPage(page).locator("#customer-receivables-receipt-files-input").setInputFiles(
        fileURLToPath(new URL("../fixtures/sample-contract.pdf", import.meta.url)),
    )
}

/** 出纳在客户往来页登记回款并核销指定销售单。 */
export async function registerCustomerReceiptForOrder(
    page: Page,
    input: {
        customerName: string
        orderNo: string
        amount?: string
        bankReference: string
    },
): Promise<{ receiptNo: string }> {
    await startReceiptSession(page, input.customerName)
    await selectReceiptAllocations(page, [input.orderNo])
    const amountInput = workPage(page).locator("#customer-receivables-session-amount")
    const allocation = receiptAllocationRow(page, input.orderNo)
    if (input.amount) {
        await amountInput.fill(input.amount)
    } else if (!(await amountInput.inputValue())) {
        const entryAmount = await allocation.getByRole("cell").nth(1).innerText()
        const matched = entryAmount.replace(/,/g, "").match(/\d+(?:\.\d+)?/)
        expect(matched, "应收行必须显示可核销金额").not.toBeNull()
        await amountInput.fill(matched![0])
    }
    await uploadReceiptEvidence(page)
    const fillLine = allocation.getByRole("button", { name: "自动填入" })
    await expect(fillLine).toBeVisible({ timeout: TIMEOUT })
    await fillLine.click()
    await workPage(page)
        .locator("#customer-receivables-session-bank-reference")
        .fill(input.bankReference)
    await workPage(page).locator("#customer-receivables-session-submit").click()
    await expect(
        workPage(page).getByRole("heading", { name: /提交回款|确认提交回款/ }),
    ).toBeVisible({
        timeout: TIMEOUT,
    })

    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/customer-receipts/commit"),
        { timeout: 60_000 },
    )
    await workPage(page)
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
        workPage(page).getByRole("dialog").locator('[data-slot="quick-preview-identity"]').getByText(`回款单：${receiptNo}`, { exact: true }),
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
        workPage(page).getByRole("dialog").getByRole("heading", { name: /客户退款/ }),
    ).toBeVisible({ timeout: TIMEOUT })
    const reasonInput = workPage(page)
        .locator("#customer-receivables-refund-request-reason")
        .or(workPage(page).locator("#customer-receivables-refund-reason"))
        .first()
    await reasonInput.fill(reason)
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/customer-refunds/commit"),
        { timeout: 60_000 },
    )
    await workPage(page).locator("#customer-receivables-refund-request-submit").click()
    const confirm = workPage(page).getByRole("alertdialog", { name: /确认提交退款|提交退款/ })
    // 确认框与提交响应谁先到走哪条；prepareRefundDraft 直接提交时不弹确认框。
    if (await optionalStepVisible(confirm, committed)) {
        await workPage(page)
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
    await expect(workPage(page).getByText("退款已提交审批").first()).toBeVisible({
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
        workPage(page).getByRole("dialog").getByRole("heading", { name: /回款冲正/ }),
    ).toBeVisible({ timeout: TIMEOUT })
    const reasonInput = workPage(page)
        .locator("#customer-receivables-reversal-request-reason")
        .or(workPage(page).locator("#customer-receivables-reversal-reason"))
        .first()
    await reasonInput.fill(reason)
    const committed = page.waitForResponse(
        (response) =>
            response.request().method() === "POST" &&
            response.url().includes("/admin/receipt-reversals/commit"),
        { timeout: 60_000 },
    )
    await workPage(page).locator("#customer-receivables-reversal-request-submit").click()
    const confirm = workPage(page).getByRole("alertdialog", { name: /确认提交冲正|提交冲正/ })
    // 确认框与提交响应谁先到走哪条；prepareReversalDraft 直接提交时不弹确认框。
    if (await optionalStepVisible(confirm, committed)) {
        await workPage(page)
            .locator("#customer-receivables-reversal-submit-confirm-dialog-confirm")
            .click({ force: true })
    }
    const response = await committed
    expect(response.ok(), await response.text()).toBeTruthy()
    await expect(workPage(page).getByText("冲正已提交审批").first()).toBeVisible({
        timeout: LONG,
    })
}
