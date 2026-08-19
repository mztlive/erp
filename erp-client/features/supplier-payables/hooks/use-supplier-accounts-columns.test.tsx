import { describe, it, expect, vi, afterEach } from 'vitest'
import { cleanup, fireEvent, render, renderHook } from '@testing-library/react'
import type { CellContext, ColumnDef } from '@tanstack/react-table'

import { useSupplierAccountsColumns } from '@/features/supplier-payables/hooks/use-supplier-accounts-columns'
import type {
    PayableRow,
    PaymentRow,
    PurchaseInvoiceRow,
    SupplierAccountsListView,
    UnallocatedRow,
} from '@/features/supplier-payables/types'

afterEach(cleanup)

const payableRow: PayableRow = {
    payableAccountId: 'pa-1',
    supplierId: 'sup-1',
    supplierName: '供应商甲',
    sourceType: 'PURCHASE_ORDER',
    sourceTypeLabel: '采购单',
    sourceDocumentId: 'po-1',
    sourceDocumentNo: 'PO-001',
    primaryEntryId: 'pe-1',
    entryLockVersion: 1,
    accountLockVersion: 1,
    grossTotal: '1200.00',
    settledTotal: '200.00',
    openTotal: '1000.00',
    invoicedTotal: '0.00',
    openInvoiceableTotal: '1000.00',
    dueDate: '2026-08-20',
    dueState: 'not_due',
    dueStateLabel: '未到期',
    status: 'OPEN',
    statusLabel: '未结',
    statusTone: 'warning',
    allowedActions: ['VIEW_DETAIL'],
    actionBlockers: [],
}

const paymentRow: PaymentRow = {
    paymentId: 'pay-1',
    paymentNo: 'FK-001',
    supplierId: 'sup-1',
    supplierName: '供应商甲',
    paidAt: '2026-08-14T08:00:00.000Z',
    amount: '1000.00',
    bankReferenceMasked: '****1234',
    allocatedTotal: '800.00',
    unallocatedAmount: '200.00',
    status: 'POSTED',
    statusLabel: '已过账',
    statusTone: 'success',
    baselineVersion: 1,
    allocations: [],
    allowedActions: ['VIEW_DETAIL', 'CONTINUE_ALLOCATE', 'REVERSE'],
    actionBlockers: [],
}

const invoiceRow: PurchaseInvoiceRow = {
    invoiceId: 'inv-1',
    invoiceCode: 'FP',
    invoiceNo: '001',
    invoiceKind: 'BLUE',
    invoiceKindLabel: '蓝票',
    supplierId: 'sup-1',
    supplierName: '供应商甲',
    invoiceDate: '2026-08-10',
    grossAmount: '1200.00',
    netAmount: '1061.95',
    taxAmount: '138.05',
    allocatedTotal: '0.00',
    unallocatedAmount: '1200.00',
    status: 'POSTED',
    statusLabel: '已登记',
    statusTone: 'success',
    allocations: [],
    allowedActions: ['VIEW_DETAIL', 'CONTINUE_ALLOCATE', 'RED_INVOICE'],
    actionBlockers: [],
}

const unallocatedPaymentRow: UnallocatedRow = {
    id: 'pay-1',
    track: 'payment',
    trackLabel: '付款',
    documentNo: 'FK-001',
    supplierId: 'sup-1',
    supplierName: '供应商甲',
    amount: '1000.00',
    unallocatedAmount: '200.00',
    occurredAt: '2026-08-14T08:00:00.000Z',
    statusLabel: '已确认',
    statusTone: 'success',
}

const unallocatedInvoiceRow: UnallocatedRow = {
    ...unallocatedPaymentRow,
    id: 'inv-1',
    track: 'purchase_invoice',
    trackLabel: '进项发票',
    documentNo: 'FP-001',
    amount: '1200.00',
    unallocatedAmount: '1200.00',
    statusLabel: '已登记',
}

const baseData: SupplierAccountsListView = {
    view: 'payable',
    metrics: {
        openPayableTotal: '0.00',
        overduePayableTotal: '0.00',
        unallocatedPaymentTotal: '0.00',
        unallocatedInvoiceTotal: '0.00',
        prepayGateBlockedCount: 0,
    },
    payables: [payableRow],
    payments: [paymentRow],
    invoices: [invoiceRow],
    unallocated: [unallocatedPaymentRow, unallocatedInvoiceRow],
    suppliers: [],
    total: 0,
    filterSummary: '应付台账',
    permissionVersion: 'pv-1',
    dataWatermark: 'wm-1',
    queriedAt: '2026-01-01T00:00:00.000Z',
    moduleAllowed: true,
    hasDataScope: true,
    canRegisterPayment: true,
    canRegisterInvoice: true,
    canExport: true,
    payablePriorityPolicy: {
        state: 'MISSING',
        mixedAutoAllocationAllowed: false,
    },
    allowFullBankReveal: false,
}

function setup(overrides: Partial<Parameters<typeof useSupplierAccountsColumns>[0]> = {}) {
    const openPreview = vi.fn()
    const openPaymentPreview = vi.fn()
    const openSession = vi.fn()
    const setReverseTarget = vi.fn()
    const setRedInvoiceNo = vi.fn()
    const input = {
        data: baseData,
        returnTo: '/finance/supplier-accounts',
        fromWorkspace: '/w/finance',
        openPreview,
        openPaymentPreview,
        openSession,
        setReverseTarget,
        setRedInvoiceNo,
        ...overrides,
    }
    const { result } = renderHook(() => useSupplierAccountsColumns(input))
    return { ...input, result }
}

function renderColumnCell<T>(
    columns: { id?: string; cell?: ColumnDef<T>['cell'] }[],
    columnId: string,
    row: T,
) {
    const column = columns.find((c) => c.id === columnId)
    if (typeof column?.cell !== 'function') {
        throw new Error(`cell renderer missing for ${columnId}`)
    }
    const element = column.cell({
        row: { original: row },
    } as CellContext<T, unknown>) as React.ReactElement
    return render(element)
}

describe('useSupplierAccountsColumns', () => {
    it('builds the four column sets with expected ids and headers', () => {
        const { result } = setup()

        expect(result.current.payableColumns.map((c) => c.id)).toEqual([
            'supplier',
            'amounts',
            'tracks',
            'due',
            'status',
            'actions',
        ])
        expect(result.current.payableColumns.map((c) => c.header)).toEqual([
            '供应商 / 来源',
            '应付（含税）/ 开放（含税）',
            '已付（净）/ 已收票（净）',
            '到期',
            '状态',
            '操作',
        ])
        expect(result.current.paymentColumns.map((c) => c.id)).toEqual([
            'doc',
            'amount',
            'bank',
            'status',
            'time',
            'actions',
        ])
        expect(result.current.paymentColumns.map((c) => c.header)).toEqual([
            '付款单',
            '金额 / 未分配',
            '银行引用',
            '状态',
            '付款时间',
            '操作',
        ])
        expect(result.current.invoiceColumns.map((c) => c.id)).toEqual([
            'doc',
            'amount',
            'alloc',
            'status',
            'actions',
        ])
        expect(result.current.invoiceColumns.map((c) => c.header)).toEqual([
            '进项发票',
            '含税 / 未分配',
            '净已分配',
            '状态',
            '操作',
        ])
        expect(result.current.unallocatedColumns.map((c) => c.id)).toEqual([
            'track',
            'doc',
            'amount',
            'actions',
        ])
        expect(result.current.unallocatedColumns.map((c) => c.header)).toEqual([
            '轨道',
            '单据 / 供应商',
            '未分配余额',
            '操作',
        ])
    })

    it('keeps column arrays stable across re-renders when deps are unchanged (memoized)', () => {
        const input = {
            data: baseData,
            openPreview: vi.fn(),
            openPaymentPreview: vi.fn(),
            openSession: vi.fn(),
            setReverseTarget: vi.fn(),
            setRedInvoiceNo: vi.fn(),
        }
        const { result, rerender } = renderHook(() =>
            useSupplierAccountsColumns(input),
        )
        const first = result.current
        rerender()
        expect(result.current.payableColumns).toBe(first.payableColumns)
        expect(result.current.paymentColumns).toBe(first.paymentColumns)
        expect(result.current.invoiceColumns).toBe(first.invoiceColumns)
        expect(result.current.unallocatedColumns).toBe(first.unallocatedColumns)
    })

    it('renders the payable supplier cell with source type and document number', () => {
        const { result } = setup()
        const screen = renderColumnCell(
            result.current.payableColumns,
            'supplier',
            payableRow,
        )
        expect(screen.getByText('供应商甲')).toBeTruthy()
        expect(screen.getByText('PO-001')).toBeTruthy()
    })

    it('opens the preview dialog from the payable actions cell', () => {
        const { result, openPreview } = setup()
        const screen = renderColumnCell(
            result.current.payableColumns,
            'actions',
            payableRow,
        )
        fireEvent.click(screen.getByText('预览'))
        expect(openPreview).toHaveBeenCalledWith('pa-1')
    })

    it('opens the payment session with preselection and purchase order from the payable actions cell', () => {
        const { result, openSession } = setup()
        const screen = renderColumnCell(
            result.current.payableColumns,
            'actions',
            payableRow,
        )
        fireEvent.click(screen.getByText('核销付款'))
        expect(openSession).toHaveBeenCalledWith({
            track: 'payment',
            supplierId: 'sup-1',
            preselectPayableAccountId: 'pa-1',
            purchaseOrderId: 'po-1',
            returnTo: '/finance/supplier-accounts',
            fromWorkspace: '/w/finance',
        })
    })

    it('disables 核销付款 when payment registration is not allowed', () => {
        const { result } = setup({
            data: { ...baseData, canRegisterPayment: false },
        })
        const screen = renderColumnCell(
            result.current.payableColumns,
            'actions',
            payableRow,
        )
        const button = screen.getByText('核销付款')
        expect(button.closest('button')?.disabled).toBe(true)
        expect(button.closest('button')?.getAttribute('title')).toBe(
            '当前无付款登记/核销权限',
        )
    })

    it('renders the payment gate hint when a payable has a payment gate summary', () => {
        const row: PayableRow = {
            ...payableRow,
            paymentGateSummary: {
                state: 'SATISFIED',
                message: '',
                required: '100.00',
                allocated: '100.00',
                gap: '0.00',
            },
        }
        const { result } = setup()
        const screen = renderColumnCell(
            result.current.payableColumns,
            'status',
            row,
        )
        expect(screen.getByText(/先款条件/)).toBeTruthy()
        expect(screen.getByText(/已满足/)).toBeTruthy()
    })

    it('opens the payment preview from the payment actions cell', () => {
        const { result, openPaymentPreview } = setup()
        const screen = renderColumnCell(
            result.current.paymentColumns,
            'actions',
            paymentRow,
        )
        fireEvent.click(screen.getByText('查看'))
        expect(openPaymentPreview).toHaveBeenCalledWith('pay-1')
    })

    it('opens the continue-allocation session from the payment actions cell', () => {
        const { result, openSession } = setup()
        const screen = renderColumnCell(
            result.current.paymentColumns,
            'actions',
            paymentRow,
        )
        fireEvent.click(screen.getByText('继续核销'))
        expect(openSession).toHaveBeenCalledWith({
            track: 'payment',
            supplierId: 'sup-1',
            existingPaymentId: 'pay-1',
            returnTo: '/finance/supplier-accounts',
            fromWorkspace: '/w/finance',
        })
    })

    it('sets the reverse target from the payment actions cell', () => {
        const { result, setReverseTarget } = setup()
        const screen = renderColumnCell(
            result.current.paymentColumns,
            'actions',
            paymentRow,
        )
        fireEvent.click(screen.getByText('冲正'))
        expect(setReverseTarget).toHaveBeenCalledWith({
            kind: 'payment',
            id: 'pay-1',
            no: 'FK-001',
        })
    })

    it('opens the invoice continue-allocation session without extra context', () => {
        const { result, openSession } = setup()
        const screen = renderColumnCell(
            result.current.invoiceColumns,
            'actions',
            invoiceRow,
        )
        fireEvent.click(screen.getByText('继续核销'))
        expect(openSession).toHaveBeenCalledWith({
            track: 'purchase_invoice',
            supplierId: 'sup-1',
            existingInvoiceId: 'inv-1',
        })
    })

    it('prefills the red invoice number and sets the reverse target from the invoice actions cell', () => {
        const { result, setRedInvoiceNo, setReverseTarget } = setup()
        const screen = renderColumnCell(
            result.current.invoiceColumns,
            'actions',
            invoiceRow,
        )
        fireEvent.click(screen.getByText('红票'))
        expect(setRedInvoiceNo).toHaveBeenCalledWith('R001')
        expect(setReverseTarget).toHaveBeenCalledWith({
            kind: 'invoice',
            id: 'inv-1',
            no: 'FP-001',
        })
    })

    it('opens the session with the payment id for an unallocated payment row', () => {
        const { result, openSession } = setup()
        const screen = renderColumnCell(
            result.current.unallocatedColumns,
            'actions',
            unallocatedPaymentRow,
        )
        const button = screen.getByText('继续核销')
        expect(button.closest('button')?.disabled).toBe(false)
        fireEvent.click(button)
        expect(openSession).toHaveBeenCalledWith({
            track: 'payment',
            supplierId: 'sup-1',
            existingPaymentId: 'pay-1',
            existingInvoiceId: undefined,
        })
    })

    it('opens the session with the invoice id for an unallocated invoice row', () => {
        const { result, openSession } = setup()
        const screen = renderColumnCell(
            result.current.unallocatedColumns,
            'actions',
            unallocatedInvoiceRow,
        )
        fireEvent.click(screen.getByText('继续核销'))
        expect(openSession).toHaveBeenCalledWith({
            track: 'purchase_invoice',
            supplierId: 'sup-1',
            existingPaymentId: undefined,
            existingInvoiceId: 'inv-1',
        })
    })

    it('disables continue-allocation when the original document cannot be resolved', () => {
        const { result, openSession } = setup()
        const row: UnallocatedRow = {
            ...unallocatedPaymentRow,
            documentNo: 'FK-999',
        }
        const screen = renderColumnCell(
            result.current.unallocatedColumns,
            'actions',
            row,
        )
        const button = screen.getByText('继续核销')
        expect(button.closest('button')?.disabled).toBe(true)
        expect(button.closest('button')?.getAttribute('title')).toBe(
            '未找到原付款/发票，请回到对应视图操作',
        )
        fireEvent.click(button)
        expect(openSession).not.toHaveBeenCalled()
    })

    it('renders empty data without crashing and disables the unallocated button', () => {
        const { result } = setup({
            data: { ...baseData, payments: [], invoices: [] },
        })
        const screen = renderColumnCell(
            result.current.unallocatedColumns,
            'actions',
            unallocatedPaymentRow,
        )
        expect(screen.getByText('继续核销').closest('button')?.disabled).toBe(
            true,
        )
    })
})
