import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { CustomerAccountDetailPreview } from "./customer-account-detail-preview"
import { InvoiceDetailBody } from "./detail-bodies"
import { createInvoiceColumns } from "./invoice-columns"
import { CUSTOMER_ACCOUNTS_INVOICE_FORBIDDEN_ACTIONS } from "@/app/(workspace)/finance/customer-accounts/invoice-page-proof"
import { invoiceActionsExcludeApproval } from "@/features/customer-receivables/lib/invoice-no-approval"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"
import { CustomerReceivablesHeader } from "@/features/customer-receivables/pages/components/customer-receivables-header"

afterEach(() => {
    cleanup()
})

const invoiceRow = (): SalesInvoiceRow => ({
    invoiceId: "inv-1",
    invoiceNo: "FP-2026-001",
    invoiceKind: "blue",
    invoiceKindLabel: "蓝票",
    counterpartyPartyId: "p1",
    counterpartyPartyName: "主体甲",
    customerId: "c1",
    customerName: "客户甲",
    invoiceDate: "2026-01-15",
    grossAmount: "113.00",
    netAmount: "100.00",
    taxAmount: "13.00",
    allocatedTotal: "50.00",
    unallocatedAmount: "63.00",
    status: "registered",
    statusLabel: "已登记",
    statusTone: "success",
    baselineVersion: 1,
    allocations: [],
    allowedActions: ["VIEW_DETAIL", "CONTINUE_ALLOCATE", "ISSUE_RED_INVOICE"],
    actionBlockers: [],
    isPosted: true,
    canEdit: false,
    canDelete: false,
})

function expectNoApprovalUi() {
    expect(screen.queryByText("审批流程")).toBeNull()
    expect(screen.queryByText("尚未绑定审批流程")).toBeNull()
    expect(screen.queryByText("当前审批人")).toBeNull()
    expect(screen.queryByRole("button", { name: "选择流程" })).toBeNull()
    expect(screen.queryByRole("button", { name: "通过" })).toBeNull()
    expect(screen.queryByRole("button", { name: "驳回" })).toBeNull()
    expect(screen.queryByRole("button", { name: "撤回审批" })).toBeNull()
    expect(screen.queryByRole("button", { name: "改派当前审批人" })).toBeNull()
    expect(screen.queryByRole("button", { name: "恢复当前审批人" })).toBeNull()
    expect(screen.queryByRole("button", { name: "取消受阻审批" })).toBeNull()
    expect(
        screen.queryByRole("button", { name: "更新审批流程版本" }),
    ).toBeNull()
    for (const label of CUSTOMER_ACCOUNTS_INVOICE_FORBIDDEN_ACTIONS) {
        expect(screen.queryByRole("button", { name: label })).toBeNull()
    }
}

describe("InvoiceDetailBody", () => {
    it("prints invoice facts and does not render the approval zone", () => {
        render(<InvoiceDetailBody row={invoiceRow()} />)
        expect(screen.getByText("发票号码")).toBeTruthy()
        expect(screen.getByText("FP-2026-001")).toBeTruthy()
        expect(screen.getByText("蓝票")).toBeTruthy()
        expect(screen.getByText("已登记发票只读")).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("CustomerAccountDetailPreview invoice path", () => {
    it("shows invoice preview actions without process selection or decisions", () => {
        render(
            <CustomerAccountDetailPreview
                open
                data={{
                    kind: "invoice",
                    invoice: invoiceRow(),
                    queriedAt: "2026-01-15T00:00:00.000Z",
                }}
                isPending={false}
                isError={false}
                error={null}
                onRetry={vi.fn()}
                onClose={vi.fn()}
                onStartSession={vi.fn()}
                onRequestReverse={vi.fn()}
            />,
        )
        expect(screen.getAllByText("FP-2026-001").length).toBeGreaterThan(0)
        expect(screen.getByRole("button", { name: "继续分配" })).toBeTruthy()
        expect(screen.getByRole("button", { name: "红票" })).toBeTruthy()
        expectNoApprovalUi()
    })
})

describe("invoice list and register entries", () => {
    it("only offers preview and continue allocate on invoice columns", () => {
        const columns = createInvoiceColumns({
            onPreview: vi.fn(),
            onStartSession: vi.fn(),
        })
        const actions = columns.find((column) => column.id === "actions")
        expect(actions).toBeTruthy()
        const row = { original: invoiceRow() }
        render(
            <div>
                {typeof actions?.cell === "function"
                    ? actions.cell({
                          row,
                      } as never)
                    : null}
            </div>,
        )
        expect(screen.getByRole("button", { name: "预览" })).toBeTruthy()
        expect(screen.getByRole("button", { name: "继续分配" })).toBeTruthy()
        expectNoApprovalUi()
        expect(invoiceActionsExcludeApproval(invoiceRow().allowedActions)).toBe(
            true,
        )
    })

    it("registers an invoice without offering process selection", () => {
        render(
            <CustomerReceivablesHeader
                data={{
                    view: "sales_invoice",
                    queriedAt: "2026-01-15T00:00:00.000Z",
                    total: 1,
                    canExport: true,
                    canRegister: true,
                    moduleAllowed: true,
                    hasDataScope: true,
                    filterSummary: "销项发票",
                    metrics: {
                        openReceivableTotal: "0",
                        overdueReceivableTotal: "0",
                        unallocatedReceiptTotal: "0",
                        unallocatedInvoiceTotal: "0",
                        cardPendingReviewCount: 0,
                    },
                    receivables: [],
                    receipts: [],
                    invoices: [],
                    counterparties: [],
                    unallocated: { receipts: [], invoices: [], note: "" },
                    permissionVersion: "1",
                    dataWatermark: "2026-01-15T00:00:00.000Z",
                    submitPolicy: {
                        allowUnallocatedRemainder: true,
                        label: "允许保留未分配余额",
                    },
                }}
                onExport={vi.fn()}
                onRegisterInvoice={vi.fn()}
                onRegisterReceipt={vi.fn()}
            />,
        )
        expect(screen.getByRole("button", { name: "登记销项发票" })).toBeTruthy()
        expectNoApprovalUi()
    })
})
