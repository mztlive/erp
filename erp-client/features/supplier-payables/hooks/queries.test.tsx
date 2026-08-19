import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'

import * as payablesApi from '@/features/supplier-payables/api/requests'
import {
    useAllocationSessionQuery,
    usePayableDetailQuery,
    useResolveUnknownMutation,
    useReverseInvoiceMutation,
    useReversePaymentMutation,
    useSaveAllocationDraftMutation,
    useSubmitInvoiceMutation,
    useSubmitPaymentMutation,
    useSubmitSupplierRefundMutation,
    useSupplierAccountsQuery,
    useEnsureSupplierRefundDraftMutation,
    useSupplierRefundQuery,
} from '@/features/supplier-payables/hooks/queries'
import type {
    AllocationSessionView,
    FormalSubmitResult,
    PostInvoiceInput,
    PostPaymentInput,
    SupplierAccountsListView,
    SupplierAccountsQuery,
} from '@/features/supplier-payables/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/supplier-payables/api/requests', () => ({
    fetchAllocationSession: vi.fn(),
    fetchPayableDetail: vi.fn(),
    fetchSupplierAccounts: vi.fn(),
    resolveUnknownResult: vi.fn(),
    reverseInvoice: vi.fn(),
    reversePayment: vi.fn(),
    saveAllocationDraft: vi.fn(),
    submitInvoice: vi.fn(),
    submitPayment: vi.fn(),
    ensureSupplierPaymentDraft: vi.fn(),
    fetchSupplierPayment: vi.fn(),
    fetchSupplierRefund: vi.fn(),
    ensureSupplierRefundDraft: vi.fn(),
    submitSupplierRefund: vi.fn(),
    forgetSupplierRefundDraft: vi.fn(),
}))

const mockedApi = vi.mocked(payablesApi)

const baseQuery: SupplierAccountsQuery = { view: 'payable' }

const listView = (): SupplierAccountsListView => ({
    view: 'payable',
    metrics: {
        openPayableTotal: '0.00',
        overduePayableTotal: '0.00',
        unallocatedPaymentTotal: '0.00',
        unallocatedInvoiceTotal: '0.00',
        prepayGateBlockedCount: 0,
    },
    payables: [],
    payments: [],
    invoices: [],
    unallocated: [],
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
})

const sessionView = (): AllocationSessionView => ({
    draftSessionId: 'alloc_sup_201',
    track: 'payment',
    supplierId: 'sup-1',
    supplierName: 'sup-1',
    pool: [],
    payablePriorityPolicy: {
        state: 'MISSING',
        mixedAutoAllocationAllowed: false,
    },
    preselectedPayableAccountIds: [],
    dataWatermark: 'wm-sess-0',
    queriedAt: '2026-01-01T00:00:00.000Z',
})

const succeeded = (
    title: string,
    extra: Partial<FormalSubmitResult> = {},
): FormalSubmitResult => ({
    status: 'succeeded',
    title,
    description: 'ok',
    ...extra,
})

const failed = (): FormalSubmitResult => ({
    status: 'failed',
    title: '失败',
    description: '原因',
    errorCode: 'HTTP_ERROR',
})

const paymentInput: PostPaymentInput = {
    draftSessionId: 's1',
    supplierId: 'sup-1',
    paidAt: '2026-08-14',
    amount: '100.00',
    bankReference: '****',
    targets: [],
    explicitSelection: true,
    idempotencyKey: 'pay-key-1',
}

const invoiceInput: PostInvoiceInput = {
    draftSessionId: 's1',
    supplierId: 'sup-1',
    invoiceCode: 'FP',
    invoiceNo: '001',
    invoiceDate: '2026-08-14',
    grossAmount: '120.00',
    netAmount: '106.19',
    taxAmount: '13.81',
    invoiceKind: 'BLUE',
    targets: [],
    explicitSelection: true,
    idempotencyKey: 'inv-key-1',
}

describe('useSupplierAccountsQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the list under the stable list key', async () => {
        mockedApi.fetchSupplierAccounts.mockResolvedValue(listView())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useSupplierAccountsQuery(baseQuery),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.data).toEqual(listView()),
        )
        expect(mockedApi.fetchSupplierAccounts).toHaveBeenCalledWith(baseQuery)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['supplier-payables', 'list', baseQuery],
        ])
    })

    it('reuses the cache for a structurally equal query (key stability)', async () => {
        mockedApi.fetchSupplierAccounts.mockResolvedValue(listView())

        const client = createFreshQueryClient()
        const wrapper = ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        )
        const { result, rerender } = renderHook(
            ({ query }: { query: SupplierAccountsQuery }) =>
                useSupplierAccountsQuery(query),
            { wrapper, initialProps: { query: baseQuery } },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))

        rerender({ query: { view: 'payable' } })

        expect(result.current.data).toEqual(listView())
        expect(mockedApi.fetchSupplierAccounts).toHaveBeenCalledTimes(1)

        rerender({ query: { ...baseQuery, view: 'payment' } })
        await waitFor(() =>
            expect(mockedApi.fetchSupplierAccounts).toHaveBeenCalledTimes(2),
        )
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchSupplierAccounts.mockRejectedValue(new Error('boom'))

        const { result } = renderHookWithProviders(() =>
            useSupplierAccountsQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('usePayableDetailQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled and never fetches for a null id', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => usePayableDetailQuery(null),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchPayableDetail).not.toHaveBeenCalled()
    })

    it('fetches the detail under the detail key', async () => {
        const detail = {
            payable: { payableAccountId: 'pa-1' },
            entries: [],
            paymentAllocations: [],
            invoiceAllocations: [],
            dataWatermark: 'wm-pa-1',
            queriedAt: '2026-01-01T00:00:00.000Z',
        } as unknown as import('@/features/supplier-payables/types').PayableDetailView
        mockedApi.fetchPayableDetail.mockResolvedValue(detail)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => usePayableDetailQuery('pa-1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(detail))
        expect(mockedApi.fetchPayableDetail).toHaveBeenCalledWith('pa-1')
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['supplier-payables', 'detail', 'pa-1'],
        ])
    })

    it('surfaces null data when the api returns null', async () => {
        mockedApi.fetchPayableDetail.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            usePayableDetailQuery('missing'),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })
})

describe('useAllocationSessionQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled for null params', () => {
        const { result } = renderHookWithProviders(() =>
            useAllocationSessionQuery(null),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchAllocationSession).not.toHaveBeenCalled()
    })

    it('fetches the session with full params under the session key', async () => {
        mockedApi.fetchAllocationSession.mockResolvedValue(sessionView())

        const params = {
            track: 'payment' as const,
            supplierId: 'sup-1',
            draftSessionId: 'd1',
            returnTo: '/finance',
            fromWorkspace: '/w',
            preselectPayableAccountId: 'pa-9',
        }
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useAllocationSessionQuery(params),
            { queryClient: client },
        )

        await waitFor(() =>
            expect(result.current.data).toEqual(sessionView()),
        )
        expect(mockedApi.fetchAllocationSession).toHaveBeenCalledWith(params)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            [
                'supplier-payables',
                'session',
                {
                    track: 'payment',
                    supplierId: 'sup-1',
                    draftSessionId: 'd1',
                    purchaseOrderId: undefined,
                    existingPaymentId: undefined,
                    existingInvoiceId: undefined,
                },
            ],
        ])
    })
})

describe('useSubmitPaymentMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to submitPayment and invalidates finance sources on success', async () => {
        mockedApi.submitPayment.mockResolvedValue(succeeded('付款已确认'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitPaymentMutation(),
            { queryClient: client },
        )

        let value: FormalSubmitResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(paymentInput)
        })

        expect(mockedApi.submitPayment).toHaveBeenCalledWith(
            paymentInput,
            expect.anything(),
        )
        expect(value?.status).toBe('succeeded')
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(5))
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['supplier-payables'] }),
        )
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['purchase-orders'] }),
        )
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['fulfillment-operations'] }),
        )
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['work-items'] }),
        )
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['approval'] }),
        )
    })

    it('skips invalidation when the result is not succeeded', async () => {
        mockedApi.submitPayment.mockResolvedValue(failed())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitPaymentMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(paymentInput)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.submitPayment.mockRejectedValue(new Error('down'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitPaymentMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(paymentInput).catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useSubmitInvoiceMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to submitInvoice and invalidates finance sources on success', async () => {
        mockedApi.submitInvoice.mockResolvedValue(succeeded('进项发票已登记'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitInvoiceMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(invoiceInput)
        })

        expect(mockedApi.submitInvoice).toHaveBeenCalledWith(
            invoiceInput,
            expect.anything(),
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(5))
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['supplier-payables'] }),
        )
    })

    it('skips invalidation when the result is not succeeded', async () => {
        mockedApi.submitInvoice.mockResolvedValue(failed())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitInvoiceMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(invoiceInput)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useReversePaymentMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to reversePayment and invalidates on success', async () => {
        mockedApi.reversePayment.mockResolvedValue(succeeded('付款冲正已完成'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useReversePaymentMutation(),
            { queryClient: client },
        )

        const input = {
            paymentId: 'pay-1',
            reason: '录错',
            idempotencyKey: 'rev-pay-1',
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.reversePayment).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(5))
        expect(invalidateSpy).toHaveBeenCalledWith(
            expect.objectContaining({ queryKey: ['supplier-payables'] }),
        )
    })

    it('skips invalidation when the result is not succeeded', async () => {
        mockedApi.reversePayment.mockResolvedValue(failed())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useReversePaymentMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({
                paymentId: 'pay-1',
                reason: '录错',
                idempotencyKey: 'rev-pay-2',
            })
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useReverseInvoiceMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to reverseInvoice and invalidates on success', async () => {
        mockedApi.reverseInvoice.mockResolvedValue(succeeded('红票已登记'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useReverseInvoiceMutation(),
            { queryClient: client },
        )

        const input = {
            invoiceId: 'inv-1',
            reason: '开错',
            redInvoiceNo: 'R001',
            idempotencyKey: 'rev-inv-1',
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.reverseInvoice).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(5))
    })

    it('skips invalidation when the result is not succeeded', async () => {
        mockedApi.reverseInvoice.mockResolvedValue(failed())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useReverseInvoiceMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({
                invoiceId: 'inv-1',
                reason: '开错',
                redInvoiceNo: 'R001',
                idempotencyKey: 'rev-inv-2',
            })
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useSaveAllocationDraftMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to saveAllocationDraft without invalidation', async () => {
        mockedApi.saveAllocationDraft.mockResolvedValue({
            savedAt: '2026-08-14T08:00:00.000Z',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSaveAllocationDraftMutation(),
            { queryClient: client },
        )

        const input = {
            draftSessionId: 's1',
            track: 'payment' as const,
            supplierId: 'sup-1',
            formSnapshot: { amount: '10.00' },
        }
        let value: { savedAt: string } | undefined
        await act(async () => {
            value = await result.current.mutateAsync(input)
        })

        expect(mockedApi.saveAllocationDraft).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        expect(value?.savedAt).toBe('2026-08-14T08:00:00.000Z')
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useResolveUnknownMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to resolveUnknownResult and invalidates on a succeeded result', async () => {
        mockedApi.resolveUnknownResult.mockResolvedValue(
            succeeded('付款已确认', { operationId: 'op-1' }),
        )

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolveUnknownMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync('key-1')
        })

        expect(mockedApi.resolveUnknownResult).toHaveBeenCalledWith(
            'key-1',
            expect.anything(),
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(5))
    })

    it('does not invalidate when no result is recorded', async () => {
        mockedApi.resolveUnknownResult.mockResolvedValue(null)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolveUnknownMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync('missing')
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })

    it('does not invalidate for a failed result', async () => {
        mockedApi.resolveUnknownResult.mockResolvedValue(failed())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolveUnknownMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync('key-2')
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

const refundRow = {
    refundId: 'srf-1',
    refundNo: 'GTK-1',
    supplierId: 'sup-1',
    reasonText: '退差额',
    amount: '10.00',
    occurredAt: '',
    status: 'draft' as const,
    statusLabel: '草稿',
    statusTone: 'neutral' as const,
    baselineVersion: 1,
    allowedActions: ['VIEW_DETAIL'] as const,
    actionBlockers: [],
}

describe('useSupplierRefundQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled and never fetches for a null id', () => {
        const { result } = renderHookWithProviders(() =>
            useSupplierRefundQuery(null),
        )
        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchSupplierRefund).not.toHaveBeenCalled()
    })

    it('fetches the refund under the refund key', async () => {
        mockedApi.fetchSupplierRefund.mockResolvedValue(refundRow)
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useSupplierRefundQuery('srf-1'),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.data).toEqual(refundRow))
        expect(mockedApi.fetchSupplierRefund).toHaveBeenCalledWith('srf-1')
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['supplier-payables', 'refund', 'srf-1'],
        ])
    })
})

describe('useEnsureSupplierRefundDraftMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('invalidates refund detail and approval keys on a succeeded draft', async () => {
        mockedApi.ensureSupplierRefundDraft.mockResolvedValue({
            status: 'succeeded',
            refund: refundRow,
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useEnsureSupplierRefundDraftMutation(),
            { queryClient: client },
        )
        await result.current.mutateAsync({
            sourcePaymentId: 'sp-1',
            supplierId: 'sup-1',
            reason: '退差额',
            idempotencyKey: 'k-srf',
        })
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalled())
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['approval', 'document', 'SupplierRefund', 'srf-1'],
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['supplier-payables', 'refund', 'srf-1'],
        })
    })
})

describe('useSubmitSupplierRefundMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('invalidates payables and approval keys after submit', async () => {
        mockedApi.submitSupplierRefund.mockResolvedValue({
            status: 'succeeded',
            refund: {
                ...refundRow,
                status: 'in_approval',
                statusLabel: '审批中',
                baselineVersion: 2,
            },
        })
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitSupplierRefundMutation(),
            { queryClient: client },
        )
        await result.current.mutateAsync({
            refundId: 'srf-1',
            expectedVersion: 1,
            idempotencyKey: 'k-srf-2',
        })
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalled())
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['supplier-payables'],
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['approval', 'document', 'SupplierRefund', 'srf-1'],
        })
    })
})
