import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ReactNode } from "react"
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest"

import type {
    CustomerAccountsDetailView,
    CustomerAccountsListView,
    CustomerAccountsView,
    ReceivableAccountRow,
    ReceiptRow,
    SalesInvoiceRow,
} from "../types"
import { CustomerReceivablesTable } from "../pages/components/customer-receivables-table"
import { CustomerAccountDetailPreview } from "./customer-account-detail-preview"
import {
    createInvoiceColumns,
    createReceivableColumns,
    createReceiptColumns,
} from "./customer-account-columns"

vi.mock("@/components/business", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/components/business")>()),
    QuickPreviewSheet: ({ footer }: { footer: ReactNode }) => (
        <div>{footer}</div>
    ),
}))
vi.mock("./detail-bodies", () => ({
    ReceivableDetailBody: () => null,
    ReceiptDetailBody: () => null,
    InvoiceDetailBody: () => null,
    CustomerRefundDetailBody: () => null,
    ReceiptReversalDetailBody: () => null,
}))

beforeAll(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
        configurable: true,
        value: vi.fn(),
    })
    vi.stubGlobal(
        "ResizeObserver",
        class {
            observe() {}
            unobserve() {}
            disconnect() {}
        },
    )
})
afterEach(cleanup)

const party = {
    counterpartyPartyId: "party-1",
    counterpartyPartyName: "结算客户",
    customerId: "customer-1",
    customerName: "业务客户",
}
const receivable: ReceivableAccountRow = {
    ...party,
    accountId: "account-1",
    accountSeq: 1,
    salesOrderId: "sale-1",
    salesOrderNo: "XS-001",
    businessType: "physical_service",
    businessTypeLabel: "实物服务",
    grossTotal: "100",
    settledTotal: "0",
    openTotal: "100",
    invoicedTotal: "0",
    openInvoiceableTotal: "100",
    dueDate: "2026-09-30",
    dueState: "not_due",
    dueStateLabel: "未到期",
    status: "open",
    statusLabel: "未结",
    statusTone: "neutral",
    baselineVersion: 1,
    entries: [],
    allowedActions: ["REGISTER_RECEIPT"],
    actionBlockers: [],
}
const receipt: ReceiptRow = {
    ...party,
    receiptId: "receipt-1",
    receiptNo: "HK-001",
    receivedAt: "2026-09-09T00:00:00Z",
    amount: "100",
    bankReferenceMasked: "****",
    allocatedTotal: "0",
    unallocatedAmount: "100",
    status: "posted",
    statusLabel: "已确认",
    statusTone: "success",
    baselineVersion: 1,
    allocations: [],
    allowedActions: ["CONTINUE_ALLOCATE"],
    actionBlockers: [],
    isPosted: true,
    canEdit: false,
    canDelete: false,
}
const invoice: SalesInvoiceRow = {
    ...party,
    invoiceId: "invoice-1",
    invoiceNo: "FP-001",
    invoiceKind: "blue",
    invoiceKindLabel: "蓝票",
    invoiceDate: "2026-09-09",
    grossAmount: "100",
    netAmount: "100",
    taxAmount: "0",
    allocatedTotal: "0",
    unallocatedAmount: "100",
    status: "registered",
    statusLabel: "已登记",
    statusTone: "success",
    baselineVersion: 1,
    allocations: [],
    allowedActions: ["CONTINUE_ALLOCATE"],
    actionBlockers: [],
    isPosted: true,
    canEdit: false,
    canDelete: false,
}
const data: CustomerAccountsListView = {
    view: "receivable",
    metrics: {
        openReceivableTotal: "100",
        overdueReceivableTotal: "0",
        unallocatedReceiptTotal: "100",
        unallocatedInvoiceTotal: "100",
    },
    receivables: [receivable],
    receipts: [receipt],
    invoices: [invoice],
    unallocated: { receipts: [receipt], invoices: [invoice], note: "分别核销" },
    counterparties: [],
    total: 1,
    filterSummary: "全部",
    permissionVersion: "1",
    dataWatermark: "1",
    queriedAt: "2026-09-09",
    hasDataScope: true,
    moduleAllowed: true,
    canRegister: true,
    canExport: true,
    submitPolicy: { allowUnallocatedRemainder: true, label: "可保留余额" },
}

describe("客户往来列表行预览", () => {
    it.each([
        [
            "receivable",
            "customer-receivables-list-receivable",
            "receivable",
            "account-1",
        ],
        [
            "receipt",
            "customer-receivables-list-receipt",
            "receipt",
            "receipt-1",
        ],
        [
            "sales_invoice",
            "customer-receivables-list-invoice",
            "invoice",
            "invoice-1",
        ],
        [
            "unallocated",
            "customer-receivables-unallocated-receipts",
            "receipt",
            "receipt-1",
        ],
        [
            "unallocated",
            "customer-receivables-unallocated-invoices",
            "invoice",
            "invoice-1",
        ],
    ] as const)(
        "%s / %s 点击与 Enter 打开对应对象",
        (view, tableId, kind, id) => {
            const onPreview = vi.fn()
            render(
                <CustomerReceivablesTable
                    view={view as CustomerAccountsView}
                    data={data}
                    isPending={false}
                    isError={false}
                    error={null}
                    onRetry={vi.fn()}
                    metrics={data.metrics}
                    pagination={{ pageIndex: 0, pageSize: 20 }}
                    receivableColumns={createReceivableColumns()}
                    receiptColumns={createReceiptColumns()}
                    invoiceColumns={createInvoiceColumns()}
                    preview={{ kind, id }}
                    onPreview={onPreview}
                    toolbar={null}
                    patchUrl={vi.fn()}
                    onPaginationChange={vi.fn()}
                    clearFilters={vi.fn()}
                />,
            )
            expect(
                screen.queryByRole("columnheader", { name: /操作/ }),
            ).toBeNull()
            const row = document.getElementById(`${tableId}-row-${id}`)!
            expect(row).not.toBeNull()
            expect(row.tabIndex).toBe(0)
            fireEvent.click(row)
            expect(onPreview).toHaveBeenLastCalledWith({ kind, id })
            fireEvent.keyDown(row, { key: "Enter" })
            expect(onPreview).toHaveBeenCalledTimes(2)
            expect(onPreview).toHaveBeenLastCalledWith({ kind, id })
        },
    )
})

function renderPreview(
    detail: CustomerAccountsDetailView,
    options: {
        canStartSession?: () => boolean
        startSessionPending?: boolean
    } = {},
) {
    const onStartSession = vi.fn()
    render(
        <CustomerAccountDetailPreview
            open
            data={detail}
            isPending={false}
            isError={false}
            error={null}
            onRetry={vi.fn()}
            onClose={vi.fn()}
            onStartSession={onStartSession}
            onRequestReverse={vi.fn()}
            permissionReason="无操作权限"
            {...options}
        />,
    )
    return onStartSession
}

describe("客户往来操作移入 Sheet", () => {
    it("登记回款保留往来主体、销售单及应收子账", () => {
        const start = renderPreview({
            kind: "receivable",
            receivable,
            queriedAt: "",
        })
        fireEvent.click(screen.getByRole("button", { name: "登记回款并核销" }))
        expect(start).toHaveBeenCalledWith("receipt", "party-1", undefined, {
            salesOrderId: "sale-1",
            receivableAccountId: "account-1",
        })
    })
    it.each(["row-blocked", "no-permission", "pending"])(
        "%s 时不能从 Sheet 发起回款",
        (state) => {
            const start = renderPreview(
                {
                    kind: "receivable",
                    receivable: {
                        ...receivable,
                        allowedActions:
                            state === "row-blocked" ? [] : ["REGISTER_RECEIPT"],
                    },
                    queriedAt: "",
                },
                {
                    canStartSession: () => state !== "no-permission",
                    startSessionPending: state === "pending",
                },
            )
            const button = screen.getByRole("button", {
                name: state === "pending" ? "创建中…" : "登记回款并核销",
            }) as HTMLButtonElement
            expect(button.disabled).toBe(true)
            fireEvent.click(button)
            expect(start).not.toHaveBeenCalled()
        },
    )
    it.each(["receipt", "invoice"] as const)(
        "%s 继续分配保留已有单据编号",
        (kind) => {
            const start = renderPreview({
                kind,
                [kind]: kind === "receipt" ? receipt : invoice,
                queriedAt: "",
            })
            fireEvent.click(
                screen.getByRole("button", {
                    name: kind === "receipt" ? "继续核销" : "继续分配",
                }),
            )
            expect(start).toHaveBeenCalledWith(kind, "party-1", `${kind}-1`)
        },
    )
})

describe("客户往来原单入口", () => {
    it("应收页脚打开准确的销售单", () => {
        renderPreview({
            kind: "receivable",
            receivable: { ...receivable, salesOrderId: "sale/a" },
            queriedAt: "",
        })
        expect(
            screen
                .getByRole("button", { name: "打开销售单" })
                .getAttribute("href"),
        ).toBe("/sales/orders/sale%2Fa")
    })
    it("红票回到原发票，使用原票主键", () => {
        renderPreview({
            kind: "invoice",
            invoice: { ...invoice, originalInvoiceId: "original-invoice" },
            queriedAt: "",
        })
        const href = screen
            .getByRole("button", { name: "打开原发票" })
            .getAttribute("href")!
        expect(
            new URL(href, "http://localhost").searchParams.get("previewId"),
        ).toBe("original-invoice")
        expect(
            new URL(href, "http://localhost").searchParams.get("previewKind"),
        ).toBe("invoice")
    })
})
