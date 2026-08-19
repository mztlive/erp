import { describe, it, expect, vi, beforeEach } from 'vitest'

import * as httpApi from '@/lib/api'
import {
    fetchAllocationSession,
    fetchPayableDetail,
    fetchSupplierAccounts,
    resolveUnknownResult,
    reverseInvoice,
    reversePayment,
    saveAllocationDraft,
    submitInvoice,
    submitPayment,
} from '@/features/supplier-payables/api/requests'
import type {
    BackendInvoice,
    BackendPayableAccount,
    BackendSupplierPayment,
} from '@/features/supplier-payables/api/mappers'
import type { SupplierAccountsQuery } from '@/features/supplier-payables/types'

vi.mock('@/lib/api', () => ({
    apiGet: vi.fn(),
    apiPost: vi.fn(),
}))

const mockedHttp = vi.mocked(httpApi)

const payableAccount = (overrides: Partial<BackendPayableAccount> = {}): BackendPayableAccount => ({
    id: 'pa-1',
    source_document_id: 'po-1',
    supplier_id: 'sup-1',
    source_type: 'purchase_order',
    gross_total: '1200.00',
    settled_total: '200.00',
    open_total: '1000.00',
    invoiceable_total: '1000.00',
    invoiced_total: '0.00',
    open_invoiceable_total: '1000.00',
    status: 'open',
    version: 1,
    created_at: 1,
    entries: [
        {
            id: 'pe-1',
            entry_type: '采购',
            direction: 'increase',
            amount: '1200.00',
            due_date: '2026-08-20',
            source_document_id: 'po-1',
            source_sequence: 1,
            posted_at: 1,
        },
    ],
    ...overrides,
})

const supplierPayment = (overrides: Partial<BackendSupplierPayment> = {}): BackendSupplierPayment => ({
    id: 'pay-1',
    payment_no: 'FK-001',
    status: 'posted',
    supplier_id: 'sup-1',
    paid_at: 1,
    amount: '1000.00',
    bank_reference: '1234',
    version: 1,
    created_at: 1,
    allocated_total: '800.00',
    unallocated_amount: '200.00',
    allocations: [],
    ...overrides,
})

const backendInvoice = (overrides: Partial<BackendInvoice> = {}): BackendInvoice => ({
    id: 'inv-1',
    invoice_direction: 'purchase',
    invoice_kind: 'blue',
    party_id: 'sup-1',
    invoice_code: 'FP',
    invoice_no: '001',
    invoice_date: '2026-08-10',
    gross_amount: '1200.00',
    net_amount: '1061.95',
    tax_amount: '138.05',
    status: 'posted',
    version: 1,
    allocated_total: '0.00',
    unallocated_amount: '1200.00',
    allocations: [],
    ...overrides,
})

const query = (overrides: Partial<SupplierAccountsQuery> = {}): SupplierAccountsQuery => ({
    view: 'payable',
    ...overrides,
})

describe('fetchSupplierAccounts', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('requests all three resources with mapped params and builds the view', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce({
            items: [payableAccount()],
            total: 1,
        })
        mockedHttp.apiGet.mockResolvedValueOnce({
            items: [supplierPayment()],
            total: 1,
        })
        mockedHttp.apiGet.mockResolvedValueOnce({
            items: [backendInvoice()],
            total: 1,
        })

        const view = await fetchSupplierAccounts(
            query({ status: 'OPEN', sourceType: 'PURCHASE_ORDER' }),
        )

        expect(mockedHttp.apiGet).toHaveBeenNthCalledWith(
            1,
            '/admin/payable-accounts',
            expect.objectContaining({
                supplier_id: undefined,
                source_type: 'purchase_order',
                status: 'open',
            }),
        )
        expect(view.payables).toHaveLength(1)
        expect(view.payables[0].statusLabel).toBe('未结')
        expect(view.payments).toHaveLength(1)
        expect(view.invoices).toHaveLength(1)
        expect(view.unallocated).toHaveLength(2)
        expect(view.total).toBe(1)
        expect(view.emptyReason).toBeUndefined()
    })

    it('reports NO_DATA on an empty unfiltered list and FILTER_NO_RESULT with filters', async () => {
        mockedHttp.apiGet.mockResolvedValue({ items: [], total: 0 })

        const empty = await fetchSupplierAccounts(query())
        expect(empty.total).toBe(0)
        expect(empty.emptyReason).toBe('NO_DATA')

        vi.clearAllMocks()
        mockedHttp.apiGet.mockResolvedValue({ items: [], total: 0 })
        const filtered = await fetchSupplierAccounts(query({ q: 'xyz' }))
        expect(filtered.emptyReason).toBe('FILTER_NO_RESULT')
    })

    it('filters payables by purchase order id or document number', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce({
            items: [
                payableAccount(),
                payableAccount({
                    id: 'pa-2',
                    source_document_id: 'po-2',
                    entries: payableAccount().entries,
                }),
            ],
            total: 2,
        })
        mockedHttp.apiGet.mockResolvedValue({ items: [], total: 0 })

        const view = await fetchSupplierAccounts(
            query({ purchaseOrderId: 'po-1' }),
        )
        expect(view.payables.map((p) => p.payableAccountId)).toEqual(['pa-1'])
    })
})

describe('fetchPayableDetail', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('returns null when the detail request fails', async () => {
        mockedHttp.apiGet.mockRejectedValue(new Error('404'))

        await expect(fetchPayableDetail('pa-1')).resolves.toBeNull()
    })

    it('projects the payable and its invoice allocations', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce(payableAccount())
        mockedHttp.apiGet.mockResolvedValueOnce({
            items: [
                {
                    id: 'ia-1',
                    invoice_id: 'inv-1',
                    allocation_seq: 1,
                    allocation_action: 'apply',
                    payable_account_id: 'pa-1',
                    allocated_gross_amount: '10.00',
                    allocated_net_amount: '9.00',
                    allocated_tax_amount: '1.00',
                },
            ],
            total: 1,
        })

        const detail = await fetchPayableDetail('pa-1')

        expect(detail?.payable.payableAccountId).toBe('pa-1')
        expect(detail?.entries).toHaveLength(1)
        expect(detail?.invoiceAllocations).toHaveLength(1)
        expect(detail?.invoiceAllocations[0].action).toBe('APPLY')
    })
})

describe('fetchAllocationSession', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('builds a pool from payable accounts and stores the session for reuse', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce({
            items: [payableAccount()],
            total: 1,
        })

        const view = await fetchAllocationSession({
            track: 'payment',
            supplierId: 'sup-1',
            draftSessionId: 'sess-a',
        })
        expect(view.draftSessionId).toBe('sess-a')
        expect(view.pool).toHaveLength(1)
        expect(view.pool[0].payableAccountId).toBe('pa-1')
        expect(view.pool[0].primaryEntryId).toBe('pe-1')

        // Reused from the session map without another HTTP round trip.
        vi.clearAllMocks()
        const again = await fetchAllocationSession({
            track: 'payment',
            supplierId: 'sup-1',
            draftSessionId: 'sess-a',
        })
        expect(again).toBe(view)
        expect(mockedHttp.apiGet).not.toHaveBeenCalled()
    })

    it('hydrates existing amounts from an existing payment', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce({ items: [], total: 0 })
        mockedHttp.apiGet.mockResolvedValueOnce(supplierPayment())

        const view = await fetchAllocationSession({
            track: 'payment',
            supplierId: 'sup-1',
            existingPaymentId: 'pay-1',
            draftSessionId: 'sess-b',
        })

        expect(view.existingAmount).toBe('1000.00')
        expect(view.existingUnallocated).toBe('200.00')
        expect(view.existingDocumentNo).toBe('FK-001')
    })
})

describe('submitPayment', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('creates a draft then submits approval without posting', async () => {
        mockedHttp.apiPost.mockResolvedValueOnce(supplierPayment({ status: 'draft' }))
        mockedHttp.apiPost.mockResolvedValueOnce(
            supplierPayment({ status: 'IN_APPROVAL' }),
        )

        const result = await submitPayment({
            draftSessionId: 'sess-p1',
            supplierId: 'sup-1',
            paidAt: '2026-08-14',
            amount: '100.00',
            bankReference: 'ref',
            targets: [
                {
                    payableAccountId: 'pa-1',
                    amount: '50.00',
                    entryLockVersion: 1,
                    accountLockVersion: 1,
                },
            ],
            explicitSelection: true,
            idempotencyKey: 'test-pay-success',
        })

        expect(result.status).toBe('succeeded')
        expect(result.title).toBe('付款已提交审批')
        expect(result.documentNo).toBe('FK-001')
        expect(result.subjectStatus).toBe('IN_APPROVAL')
        expect(mockedHttp.apiPost).toHaveBeenNthCalledWith(
            1,
            '/admin/supplier-payments',
            expect.objectContaining({
                supplier_id: 'sup-1',
                amount: '100.00',
            }),
        )
        expect(mockedHttp.apiPost).toHaveBeenNthCalledWith(
            2,
            '/admin/supplier-payments/pay-1/submit',
            expect.objectContaining({
                expected_version: 1,
                idempotency_key: 'test-pay-success',
                allocations: [
                    {
                        payable_entry_id: 'pa-1',
                        allocated_amount: '50.00',
                    },
                ],
            }),
        )
        expect(mockedHttp.apiPost.mock.calls.some((call) =>
            String(call[0]).includes('/post'),
        )).toBe(false)
    })

    it('returns failure with the api error message on failure', async () => {
        mockedHttp.apiPost.mockRejectedValue(new Error('余额不足'))

        const result = await submitPayment({
            draftSessionId: 'sess-p2',
            supplierId: 'sup-1',
            paidAt: '',
            amount: '100.00',
            bankReference: '',
            targets: [
                {
                    payableAccountId: 'pa-1',
                    amount: '50.00',
                    entryLockVersion: 1,
                    accountLockVersion: 1,
                },
            ],
            explicitSelection: true,
            idempotencyKey: 'test-pay-fail',
        })

        expect(result.status).toBe('failed')
        expect(result.description).toBe('余额不足')
        expect(result.errorCode).toBe('HTTP_ERROR')
    })

    it('rejects submit when no positive allocations remain', async () => {
        const result = await submitPayment({
            draftSessionId: 'sess-p3',
            supplierId: 'sup-1',
            paidAt: '',
            amount: '100.00',
            bankReference: '',
            targets: [
                {
                    payableAccountId: 'pa-1',
                    amount: '0.00',
                    entryLockVersion: 1,
                    accountLockVersion: 1,
                },
            ],
            explicitSelection: true,
            idempotencyKey: 'test-pay-draft',
        })

        expect(result.status).toBe('failed')
        expect(result.errorCode).toBe('NEED_ALLOCATION')
        expect(result.description).toBe('提交审批至少需要一条核销分配。')
        expect(mockedHttp.apiPost).not.toHaveBeenCalled()
    })
})

describe('submitInvoice', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('returns BACKEND_GAP when continuing an existing invoice', async () => {
        const result = await submitInvoice({
            draftSessionId: 'sess-i1',
            supplierId: 'sup-1',
            invoiceCode: 'FP',
            invoiceNo: '001',
            invoiceDate: '2026-08-14',
            grossAmount: '100.00',
            netAmount: '90.00',
            taxAmount: '10.00',
            invoiceKind: 'BLUE',
            existingInvoiceId: 'inv-1',
            targets: [],
            explicitSelection: true,
            idempotencyKey: 'test-inv-gap',
        })

        expect(result.status).toBe('failed')
        expect(result.errorCode).toBe('BACKEND_GAP')
        expect(mockedHttp.apiPost).not.toHaveBeenCalled()
    })

    it('requires at least one allocation line', async () => {
        const result = await submitInvoice({
            draftSessionId: 'sess-i2',
            supplierId: 'sup-1',
            invoiceCode: 'FP',
            invoiceNo: '002',
            invoiceDate: '2026-08-14',
            grossAmount: '100.00',
            netAmount: '90.00',
            taxAmount: '10.00',
            invoiceKind: 'BLUE',
            targets: [],
            explicitSelection: true,
            idempotencyKey: 'test-inv-empty',
        })

        expect(result.status).toBe('failed')
        expect(result.errorCode).toBe('ALLOCATION_REQUIRED')
    })

    it('posts the invoice allocation and returns success', async () => {
        mockedHttp.apiPost.mockResolvedValueOnce({
            invoice_id: 'inv-9',
            invoice_no: '009',
            gross_amount: '100.00',
            allocations: [],
        })

        const result = await submitInvoice({
            draftSessionId: 'sess-i3',
            supplierId: 'sup-1',
            invoiceCode: 'FP',
            invoiceNo: '009',
            invoiceDate: '2026-08-14',
            grossAmount: '100.00',
            netAmount: '90.00',
            taxAmount: '10.00',
            invoiceKind: 'BLUE',
            targets: [
                {
                    payableAccountId: 'pa-1',
                    amount: '100.00',
                    entryLockVersion: 1,
                    accountLockVersion: 1,
                },
            ],
            explicitSelection: true,
            idempotencyKey: 'test-inv-ok',
        })

        expect(result.status).toBe('succeeded')
        expect(result.documentNo).toBe('009')
    })
})

describe('reversePayment', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('creates a reversal draft with binding and never posts', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce(supplierPayment())
        mockedHttp.apiPost.mockResolvedValueOnce({
            id: 'r1',
            reversal_no: 'PCZ-1',
            status: 'draft',
            original_supplier_payment_id: 'pay-1',
            reason_text: '录错',
            amount: '100.00',
            handled_by: 'finance_handler',
            reviewed_by: 'finance_reviewer',
            occurred_at: 1_700_000_000,
            version: 1,
            created_at: 1_700_000_000,
            approval: {
                requirement: 'PROCESS_REQUIRED',
                definition: {
                    id: 'def-pr-1',
                    name: '付款冲正审批',
                    version: 1,
                    nodes: [{ key: 'n1', name: '冲正复核', assignee_name: '张三' }],
                },
                allowed_actions: ['SUBMIT'],
            },
        })

        const result = await reversePayment({
            paymentId: 'pay-1',
            reason: '录错',
            idempotencyKey: 'test-rev-pay-ok',
        })

        expect(result.status).toBe('succeeded')
        expect(result.documentNo).toBe('PCZ-1')
        expect(result.approval?.definition?.name).toBe('付款冲正审批')
        expect(mockedHttp.apiPost).toHaveBeenNthCalledWith(
            1,
            '/admin/payment-reversals',
            expect.objectContaining({
                original_supplier_payment_id: 'pay-1',
                reason_text: '录错',
            }),
        )
        expect(
            mockedHttp.apiPost.mock.calls.some(
                ([path]) => typeof path === 'string' && path.endsWith('/post'),
            ),
        ).toBe(false)
    })
})

describe('reverseInvoice', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fails when the original invoice has no apply allocations', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce(backendInvoice())

        const result = await reverseInvoice({
            invoiceId: 'inv-1',
            reason: '开错',
            redInvoiceNo: 'R001',
            idempotencyKey: 'test-rev-inv-none',
        })

        expect(result.status).toBe('failed')
        expect(result.errorCode).toBe('NO_ALLOCATIONS')
    })

    it('issues a red invoice with reversed allocations and returns success', async () => {
        mockedHttp.apiGet.mockResolvedValueOnce(
            backendInvoice({
                allocations: [
                    {
                        id: 'ia-1',
                        allocation_seq: 1,
                        allocation_action: 'apply',
                        payable_account_id: 'pa-1',
                        allocated_gross_amount: '100.00',
                    },
                ],
            }),
        )
        mockedHttp.apiPost.mockResolvedValueOnce(
            backendInvoice({ invoice_no: 'R001', invoice_kind: 'red' }),
        )

        const result = await reverseInvoice({
            invoiceId: 'inv-1',
            reason: '开错',
            redInvoiceNo: 'R001',
            idempotencyKey: 'test-rev-inv-ok',
        })

        expect(result.status).toBe('succeeded')
        expect(result.documentNo).toBe('R001')
        expect(mockedHttp.apiPost).toHaveBeenCalledWith(
            '/admin/invoices/inv-1/red-issue',
            expect.objectContaining({
                invoice_no: 'R001',
                allocations: [
                    {
                        reverses_allocation_id: 'ia-1',
                        allocated_gross_amount: '100.00',
                        allocated_net_amount: '100.00',
                        allocated_tax_amount: '0',
                    },
                ],
            }),
        )
    })
})

describe('saveAllocationDraft and resolveUnknownResult', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('saves a draft snapshot and reports the saved time', async () => {
        const { savedAt } = await saveAllocationDraft({
            draftSessionId: 'sess-d1',
            track: 'payment',
            supplierId: 'sup-1',
            formSnapshot: { amount: '10.00' },
        })
        expect(new Date(savedAt).getTime()).not.toBeNaN()
    })

    it('resolves to null when no idempotent result is recorded', async () => {
        await expect(
            resolveUnknownResult('test-never-recorded'),
        ).resolves.toBeNull()
    })

    it('returns the recorded result after a successful submit', async () => {
        mockedHttp.apiPost.mockResolvedValueOnce(supplierPayment({ status: 'draft' }))
        mockedHttp.apiPost.mockResolvedValueOnce(
            supplierPayment({ status: 'IN_APPROVAL' }),
        )
        const first = await submitPayment({
            draftSessionId: 'sess-p4',
            supplierId: 'sup-1',
            paidAt: '',
            amount: '100.00',
            bankReference: '',
            targets: [
                {
                    payableAccountId: 'pa-1',
                    amount: '50.00',
                    entryLockVersion: 1,
                    accountLockVersion: 1,
                },
            ],
            explicitSelection: true,
            idempotencyKey: 'test-resolve-key',
        })
        const resolved = await resolveUnknownResult('test-resolve-key')
        expect(first.status).toBe('succeeded')
        expect(resolved).toEqual(first)
    })
})
