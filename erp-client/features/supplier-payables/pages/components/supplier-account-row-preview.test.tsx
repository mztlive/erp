import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import type { ReactNode } from "react"
import type { UseQueryResult } from "@tanstack/react-query"
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest"
import { SupplierAccountsTable } from "./supplier-accounts-table"
import {
    SupplierAccountsPreview,
    type SupplierAccountsPreviewProps,
} from "./supplier-accounts-preview"
import {
    SupplierAccountRecordPreview,
    type SupplierAccountRecordPreviewProps,
} from "./supplier-account-record-preview"
import type {
    PaymentRow,
    PayableRow,
    PurchaseInvoiceRow,
    UnallocatedRow,
    SupplierAccountsListView,
    SupplierAccountsView,
} from "../../types"

vi.mock("@/components/business", async (importOriginal) => ({
    ...(await importOriginal<typeof import("@/components/business")>()),
    QuickPreviewSheet: ({
        footer,
        children,
        open,
    }: {
        footer: ReactNode
        children: ReactNode
        open: boolean
    }) =>
        open ? (
            <div>
                {children}
                {footer}
            </div>
        ) : null,
}))
vi.mock("../../components/supplier-payment-detail-dialog", () => ({
    SupplierPaymentDetailDialog: ({
        onOpenChange,
    }: {
        onOpenChange: (value: boolean) => void
    }) => <button onClick={() => onOpenChange(false)}>返回付款预览</button>,
}))
vi.mock("../../components/supplier-source-documents", () => ({
    SupplierSourceDocuments: () => null,
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
const payment: PaymentRow = {
    paymentId: "pay-1",
    paymentNo: "FK-1",
    supplierId: "sup-1",
    supplierName: "华东供应商",
    paidAt: "2026-01-01T00:00:00.000Z",
    amount: "10.00",
    bankReferenceMasked: "****1234",
    allocatedTotal: "10.00",
    unallocatedAmount: "0.00",
    status: "POSTED",
    statusLabel: "已过账",
    statusTone: "success",
    baselineVersion: 1,
    allocations: [],
    allowedActions: ["VIEW_DETAIL", "REVERSE", "REFUND"],
    actionBlockers: [],
    relatedReversals: [],
}

const payable: PayableRow = {
    payableAccountId: "pa-1",
    supplierId: "sup-1",
    supplierName: "华东供应商",
    sourceType: "PURCHASE_ORDER",
    sourceTypeLabel: "采购单",
    sourceDocumentId: "po-1",
    sourceDocumentNo: "PO-1001",
    sourceHref: "/procurement/orders/po-1",
    primaryEntryId: "pe-1",
    entryLockVersion: 1,
    accountLockVersion: 1,
    grossTotal: "20.00",
    settledTotal: "8.00",
    openTotal: "12.00",
    invoicedTotal: "4.00",
    openInvoiceableTotal: "16.00",
    dueDate: "2026-09-01",
    dueState: "overdue",
    dueStateLabel: "已到期",
    status: "PARTIAL",
    statusLabel: "部分结清",
    statusTone: "warning",
    paymentGateSummary: {
        state: "BLOCKED",
        message: "先款未达门槛",
        required: "10.00",
        allocated: "8.00",
        gap: "2.00",
    },
    allowedActions: ["VIEW_DETAIL"],
    actionBlockers: [],
}

const invoice: PurchaseInvoiceRow = {
    invoiceId: "inv-1",
    invoiceCode: "CODE",
    invoiceNo: "123",
    invoiceKind: "BLUE",
    invoiceKindLabel: "蓝票",
    supplierId: "sup-1",
    supplierName: "华东供应商",
    invoiceDate: "2026-09-09",
    grossAmount: "100",
    netAmount: "90",
    taxAmount: "10",
    allocatedTotal: "20",
    unallocatedAmount: "80",
    status: "POSTED",
    statusLabel: "已过账",
    statusTone: "success",
    allocations: [],
    allowedActions: ["CONTINUE_ALLOCATE", "RED_INVOICE"],
    actionBlockers: [],
}
const unallocated: UnallocatedRow = {
    id: invoice.invoiceId,
    track: "purchase_invoice",
    trackLabel: "进项发票",
    documentNo: invoice.invoiceNo,
    supplierId: invoice.supplierId,
    supplierName: invoice.supplierName,
    amount: invoice.grossAmount,
    unallocatedAmount: invoice.unallocatedAmount,
    occurredAt: invoice.invoiceDate,
    statusLabel: invoice.statusLabel,
    statusTone: invoice.statusTone,
}
const data: SupplierAccountsListView = {
    view: "payable",
    metrics: {
        openPayableTotal: "12",
        overduePayableTotal: "12",
        unallocatedPaymentTotal: "0",
        unallocatedInvoiceTotal: "80",
        prepayGateBlockedCount: 0,
    },
    payables: [payable],
    payments: [payment],
    invoices: [invoice],
    unallocated: [unallocated],
    suppliers: [],
    total: 1,
    filterSummary: "",
    permissionVersion: "1",
    dataWatermark: "1",
    queriedAt: "2026-09-09",
    moduleAllowed: true,
    hasDataScope: true,
    canRegisterPayment: true,
    canRegisterInvoice: true,
    canExport: true,
    payablePriorityPolicy: {
        state: "AVAILABLE",
        mixedAutoAllocationAllowed: true,
    },
    allowFullBankReveal: false,
}
function recordProps(
    overrides: Partial<SupplierAccountRecordPreviewProps> = {},
): SupplierAccountRecordPreviewProps {
    return {
        kind: "invoice",
        onClose: vi.fn(),
        onOpenPayable: vi.fn(),
        onOpenReversal: vi.fn(),
        onOpenSession: vi.fn(),
        onReverse: vi.fn(),
        onRedInvoiceNo: vi.fn(),
        onRefund: vi.fn(),
        ...overrides,
    }
}
function query<T>(value: T): UseQueryResult<T, Error> {
    return {
        data: value,
        isPending: false,
        isError: false,
        refetch: vi.fn(),
    } as unknown as UseQueryResult<T, Error>
}
function previewProps(
    overrides: Partial<SupplierAccountsPreviewProps> = {},
): SupplierAccountsPreviewProps {
    return {
        ...recordProps(),
        previewInvoiceId: null,
        previewUnallocatedId: null,
        previewPayableId: null,
        previewPaymentId: null,
        previewRefundId: null,
        previewReversalId: null,
        listData: data,
        listLoading: false,
        onRetryList: vi.fn(),
        detailQuery: query(null),
        paymentQuery: query(null),
        refundQuery: query(null),
        reversalQuery: query(null),
        returnTo: "/workspace",
        fromWorkspace: "W12",
        ...overrides,
    }
}
describe("供应商往来行预览", () => {
    it.each([
        ["payable", payable, payable.payableAccountId],
        ["payment", payment, payment.paymentId],
        ["purchase_invoice", invoice, invoice.invoiceId],
        ["unallocated", unallocated, unallocated.id],
    ] as const)("%s 表无操作列，点击和 Enter 打开对应记录", (view, row, id) => {
        const open = vi.fn()
        render(
            <SupplierAccountsTable
                view={view as SupplierAccountsView}
                onViewChange={vi.fn()}
                data={data}
                pageRows={[row]}
                rowCount={1}
                loading={false}
                isError={false}
                error={null}
                onRetry={vi.fn()}
                pagination={{ pageIndex: 0, pageSize: 20 }}
                onPaginationChange={vi.fn()}
                sorting={[]}
                onSortingChange={vi.fn()}
                onClearFilters={vi.fn()}
                previewRowId={id}
                openPreview={open}
                openPaymentPreview={open}
                openInvoicePreview={open}
                openUnallocatedPreview={open}
                openReversalPreview={vi.fn()}
                toolbar={null}
            />,
        )
        expect(screen.queryByRole("columnheader", { name: /操作/ })).toBeNull()
        const target = document.querySelector(`[data-row-id="${id}"]`)!
        fireEvent.click(target)
        fireEvent.keyDown(target, { key: "Enter" })
        expect(open).toHaveBeenNthCalledWith(1, id)
        expect(open).toHaveBeenNthCalledWith(2, id)
    })
    it("付款冲正与退款携带原付款和金额", () => {
        const props = recordProps({ kind: "payment", payment })
        render(<SupplierAccountRecordPreview {...props} />)
        fireEvent.click(screen.getByRole("button", { name: "冲正" }))
        expect(props.onReverse).toHaveBeenCalledWith({
            kind: "payment",
            id: payment.paymentId,
            no: payment.paymentNo,
            amount: payment.amount,
            supplierName: payment.supplierName,
        })
        fireEvent.click(screen.getByRole("button", { name: "退款" }))
        expect(props.onRefund).toHaveBeenCalledWith({
            sourcePaymentId: payment.paymentId,
            sourcePaymentNo: payment.paymentNo,
            supplierId: payment.supplierId,
            supplierName: payment.supplierName,
            amount: payment.amount,
        })
    })
    it("付款详情关闭后返回轻预览", () => {
        render(
            <SupplierAccountRecordPreview
                {...recordProps({ kind: "payment", payment })}
            />,
        )
        fireEvent.click(screen.getByRole("button", { name: "查看详情" }))
        expect(screen.queryByRole("button", { name: "退款" })).toBeNull()
        fireEvent.click(screen.getByRole("button", { name: "返回付款预览" }))
        expect(screen.getByRole("button", { name: "退款" })).toBeTruthy()
    })
    it("红票和继续核销保留原发票 ID", () => {
        const props = recordProps({ invoice })
        render(<SupplierAccountRecordPreview {...props} />)
        fireEvent.click(screen.getByRole("button", { name: "红票" }))
        expect(props.onRedInvoiceNo).toHaveBeenCalledWith("R123")
        expect(props.onReverse).toHaveBeenCalledWith({
            kind: "invoice",
            id: invoice.invoiceId,
            no: "CODE-123",
        })
        fireEvent.click(screen.getByRole("button", { name: "继续核销" }))
        expect(props.onOpenSession).toHaveBeenCalledWith({
            track: "purchase_invoice",
            supplierId: invoice.supplierId,
            existingInvoiceId: invoice.invoiceId,
        })
    })
    it("没有许可时不显示红票、冲正、退款或核销入口", () => {
        render(
            <SupplierAccountRecordPreview
                {...recordProps({
                    payment: { ...payment, allowedActions: [] },
                    invoice: { ...invoice, allowedActions: [] },
                })}
            />,
        )
        for (const name of ["红票", "冲正", "退款", "继续核销"])
            expect(screen.queryByRole("button", { name })).toBeNull()
    })
    it("待核销按原发票 ID 关联，不受展示号码格式影响", () => {
        const props = previewProps({ previewUnallocatedId: unallocated.id })
        render(<SupplierAccountsPreview {...props} />)
        fireEvent.click(screen.getByRole("button", { name: "继续核销" }))
        expect(props.onOpenSession).toHaveBeenCalledWith({
            track: "purchase_invoice",
            supplierId: invoice.supplierId,
            existingInvoiceId: invoice.invoiceId,
        })
    })
    it.each(["payment", "missing", "denied"])(
        "待核销 %s 状态禁止继续核销",
        (scenario) => {
            const props = recordProps({
                kind: "unallocated",
                unallocated: {
                    ...unallocated,
                    track:
                        scenario === "payment" ? "payment" : "purchase_invoice",
                },
                invoice:
                    scenario === "denied"
                        ? { ...invoice, allowedActions: [] }
                        : undefined,
            })
            render(<SupplierAccountRecordPreview {...props} />)
            const button = screen.getByRole("button", { name: "继续核销" })
            expect(button.hasAttribute("disabled")).toBe(true)
            fireEvent.click(button)
            expect(props.onOpenSession).not.toHaveBeenCalled()
        },
    )
    it.each([
        [false, "pa-1"],
        [true, "other"],
    ])("应付付款操作同时受权限 %s 与任务 %s 限制", (allowed, taskId) => {
        render(
            <SupplierAccountsPreview
                {...previewProps({
                    previewPayableId: payable.payableAccountId,
                    canRegisterPayment: Boolean(allowed),
                    paymentTaskPayableAccountId: String(taskId),
                    detailQuery: query({
                        payable,
                        entries: [],
                        paymentAllocations: [],
                        invoiceAllocations: [],
                        dataWatermark: "1",
                        queriedAt: "2026-09-09",
                    }),
                })}
            />,
        )
        expect(screen.queryByRole("button", { name: "登记付款" })).toBeNull()
    })
    it("当前付款任务保留应付、采购单和返回上下文", () => {
        const props = previewProps({
            previewPayableId: payable.payableAccountId,
            canRegisterPayment: true,
            paymentTaskPayableAccountId: payable.payableAccountId,
            detailQuery: query({
                payable,
                entries: [],
                paymentAllocations: [],
                invoiceAllocations: [],
                dataWatermark: "1",
                queriedAt: "2026-09-09",
            }),
        })
        render(<SupplierAccountsPreview {...props} />)
        fireEvent.click(screen.getByRole("button", { name: "登记付款" }))
        expect(props.onOpenSession).toHaveBeenCalledWith({
            track: "payment",
            supplierId: payable.supplierId,
            preselectPayableAccountId: payable.payableAccountId,
            purchaseOrderId: payable.sourceDocumentId,
            returnTo: "/workspace",
            fromWorkspace: "W12",
        })
    })
})

it("应付来源链接缺失时按原采购单 ID 提供页脚入口", () => {
    render(
        <SupplierAccountsPreview
            {...previewProps({
                previewPayableId: payable.payableAccountId,
                detailQuery: query({
                    payable: {
                        ...payable,
                        sourceHref: undefined,
                        sourceDocumentId: "po/a",
                    },
                    entries: [],
                    paymentAllocations: [],
                    invoiceAllocations: [],
                    dataWatermark: "1",
                    queriedAt: "",
                }),
            })}
        />,
    )
    expect(
        screen.getByRole("button", { name: "打开采购单" }).getAttribute("href"),
    ).toBe("/procurement/orders/po%2Fa")
})
it("供应商红票提供原票入口，原票不在列表仍按详情显示", () => {
    render(
        <SupplierAccountsPreview
            {...previewProps({
                previewInvoiceId: "original-invoice",
                listData: { ...data, invoices: [] },
                invoiceQuery: query({
                    ...invoice,
                    invoiceId: "original-invoice",
                    originalInvoiceId: "blue-1",
                }),
            })}
        />,
    )
    expect(
        screen.getByRole("button", { name: "打开原发票" }).getAttribute("href"),
    ).toContain("detailId=blue-1")
})
