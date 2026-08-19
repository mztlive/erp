import { describe, it, expect, vi, beforeEach } from 'vitest'
import { waitFor } from '@testing-library/react'

import * as customerReceivablesApi from '@/features/customer-receivables/api'
import {
    useAllocationSessionQuery,
    useCreateAllocationSessionMutation,
    useCustomerAccountsDetailQuery,
    useCustomerAccountsListQuery,
    usePostAllocationMutation,
    useResolvePostUnknownMutation,
    useReverseFactMutation,
    useSaveAllocationDraftMutation,
} from '@/features/customer-receivables/hooks/queries'
import type {
    AllocationSessionView,
    CustomerAccountsDetailView,
    CustomerAccountsListView,
    CustomerAccountsQuery,
} from '@/features/customer-receivables/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/customer-receivables/api', () => ({
    fetchCustomerAccountsList: vi.fn(),
    fetchCustomerAccountsDetail: vi.fn(),
    createAllocationSession: vi.fn(),
    fetchAllocationSession: vi.fn(),
    saveAllocationDraft: vi.fn(),
    postAllocation: vi.fn(),
    resolvePostUnknown: vi.fn(),
    reverseFact: vi.fn(),
    ensureCustomerReceiptDraft: vi.fn(),
}))

const mockedApi = vi.mocked(customerReceivablesApi)

const listQuery = (overrides: Partial<CustomerAccountsQuery> = {}): CustomerAccountsQuery => ({
    view: 'receivable',
    ...overrides,
})

const sessionView = (draftSessionId: string): AllocationSessionView => ({
    draftSessionId,
    mode: 'receipt',
    counterpartyPartyId: 'p1',
    counterpartyPartyName: '主体甲',
    customerId: 'c1',
    customerName: '客户甲',
    status: 'draft',
    fact: { receivedAt: '', amount: '100', bankReference: '' },
    pool: [],
    allocations: [],
    proposedAllocatedTotal: '0.00',
    proposedUnallocated: '100.00',
    factAmount: '100',
    submitPolicy: {
        allowUnallocatedRemainder: true,
        label: '允许保留未分配余额（系统统一判定）',
    },
    leaseValid: true,
    editVersion: 1,
    note: '',
})

describe('useCustomerAccountsListQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the list view with the query as queryFn arg and key', async () => {
        const query = listQuery({ view: 'receipt', q: 'SK' })
        const view = { view: 'receipt' } as CustomerAccountsListView
        mockedApi.fetchCustomerAccountsList.mockResolvedValue(view)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerAccountsListQuery(query),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        expect(mockedApi.fetchCustomerAccountsList).toHaveBeenCalledWith(query)

        await waitFor(() => expect(result.current.data).toEqual(view))

        const queries = client.getQueryCache().getAll()
        expect(queries).toHaveLength(1)
        expect(queries[0].queryKey).toEqual([
            'customer-receivables',
            'list',
            query,
        ])
    })

    it('propagates errors from the list loader', async () => {
        mockedApi.fetchCustomerAccountsList.mockRejectedValue(
            new Error('boom'),
        )

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerAccountsListQuery(listQuery()),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useCustomerAccountsDetailQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled and never fetches when kind or id is null', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerAccountsDetailQuery(null, null),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchCustomerAccountsDetail).not.toHaveBeenCalled()
    })

    it('fetches the detail view with the detail key', async () => {
        const detail = {
            kind: 'receipt',
            queriedAt: '2026-01-01T00:00:00.000Z',
        } as CustomerAccountsDetailView
        mockedApi.fetchCustomerAccountsDetail.mockResolvedValue(detail)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerAccountsDetailQuery('receipt', 'r1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(detail))
        expect(mockedApi.fetchCustomerAccountsDetail).toHaveBeenCalledWith(
            'receipt',
            'r1',
        )

        const queries = client.getQueryCache().getAll()
        expect(queries[0].queryKey).toEqual([
            'customer-receivables',
            'detail',
            'receipt',
            'r1',
        ])
    })

    it('returns null data without failing when the api resolves null', async () => {
        mockedApi.fetchCustomerAccountsDetail.mockResolvedValue(null)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerAccountsDetailQuery('receivable', 'missing'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })
})

describe('useAllocationSessionQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled for a null draft session id', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useAllocationSessionQuery(null),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchAllocationSession).not.toHaveBeenCalled()
    })

    it('fetches the session with the session key', async () => {
        const view = sessionView('alloc_cust_1')
        mockedApi.fetchAllocationSession.mockResolvedValue(view)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useAllocationSessionQuery('alloc_cust_1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(view))
        expect(mockedApi.fetchAllocationSession).toHaveBeenCalledWith(
            'alloc_cust_1',
        )

        const queries = client.getQueryCache().getAll()
        expect(queries[0].queryKey).toEqual([
            'customer-receivables',
            'session',
            'alloc_cust_1',
        ])
    })
})

describe('useCreateAllocationSessionMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn and invalidates the new session key on success', async () => {
        const view = sessionView('alloc_cust_9')
        mockedApi.createAllocationSession.mockResolvedValue(view)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateAllocationSessionMutation(),
            { queryClient: client },
        )

        const input = { mode: 'receipt' as const, counterpartyPartyId: 'p1' }
        const value = await result.current.mutateAsync(input)

        expect(mockedApi.createAllocationSession).toHaveBeenCalledTimes(1)
        expect(mockedApi.createAllocationSession.mock.calls[0][0]).toEqual(
            input,
        )
        expect(value).toEqual(view)

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customer-receivables', 'session', 'alloc_cust_9'],
        })
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.createAllocationSession.mockRejectedValue(
            new Error('fail'),
        )

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateAllocationSessionMutation(),
            { queryClient: client },
        )

        await expect(
            result.current.mutateAsync({
                mode: 'receipt',
                counterpartyPartyId: 'p1',
            }),
        ).rejects.toThrow('fail')
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useSaveAllocationDraftMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn and invalidates the session key on success', async () => {
        const next = { ...sessionView('alloc_cust_2'), editVersion: 2 }
        mockedApi.saveAllocationDraft.mockResolvedValue(next)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSaveAllocationDraftMutation(),
            { queryClient: client },
        )

        const input = {
            draftSessionId: 'alloc_cust_2',
            fact: { receivedAt: '', amount: '50', bankReference: '' },
            allocations: [],
            editVersion: 1,
        }
        const value = await result.current.mutateAsync(input)

        expect(mockedApi.saveAllocationDraft).toHaveBeenCalledTimes(1)
        expect(mockedApi.saveAllocationDraft.mock.calls[0][0]).toEqual(input)
        expect(value).toEqual(next)

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customer-receivables', 'session', 'alloc_cust_2'],
        })
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.saveAllocationDraft.mockRejectedValue(new Error('409'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSaveAllocationDraftMutation(),
            { queryClient: client },
        )

        await expect(
            result.current.mutateAsync({
                draftSessionId: 'alloc_cust_2',
                fact: {},
                allocations: [],
                editVersion: 1,
            }),
        ).rejects.toThrow('409')
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('usePostAllocationMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn and invalidates everything on success', async () => {
        mockedApi.postAllocation.mockResolvedValue({
            status: 'succeeded',
            mode: 'receipt',
            factId: 'r1',
            factNo: 'SK-1',
            allocatedTotal: '10.00',
            unallocatedAmount: '90.00',
            operationId: 'op1',
            watermark: '2026-01-01T00:00:00.000Z',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => usePostAllocationMutation(),
            { queryClient: client },
        )

        const input = {
            draftSessionId: 'alloc_cust_3',
            editVersion: 1,
            idempotencyKey: 'k1',
        }
        await result.current.mutateAsync(input)

        expect(mockedApi.postAllocation).toHaveBeenCalledTimes(1)
        expect(mockedApi.postAllocation.mock.calls[0][0]).toEqual(input)

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(2))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customer-receivables'],
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['approval', 'document', 'CustomerReceipt', 'r1'],
        })
    })

    it('does not invalidate when the post failed', async () => {
        mockedApi.postAllocation.mockResolvedValue({
            status: 'failed',
            code: 'SESSION_INVALID',
            message: '本次核销已不存在或已提交。',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => usePostAllocationMutation(),
            { queryClient: client },
        )

        await result.current.mutateAsync({
            draftSessionId: 'alloc_cust_3',
            editVersion: 1,
            idempotencyKey: 'k2',
        })
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useResolvePostUnknownMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn and invalidates on a succeeded resolution', async () => {
        mockedApi.resolvePostUnknown.mockResolvedValue({
            status: 'succeeded',
            mode: 'invoice',
            factId: 'i1',
            factNo: 'FP-1',
            allocatedTotal: '0.00',
            unallocatedAmount: '0.00',
            operationId: 'op1',
            watermark: '2026-01-01T00:00:00.000Z',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolvePostUnknownMutation(),
            { queryClient: client },
        )

        await result.current.mutateAsync('k1')

        expect(mockedApi.resolvePostUnknown).toHaveBeenCalledTimes(1)
        expect(mockedApi.resolvePostUnknown.mock.calls[0][0]).toBe('k1')

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customer-receivables'],
        })
    })

    it('does not invalidate when the resolution is still unknown', async () => {
        mockedApi.resolvePostUnknown.mockResolvedValue({
            status: 'unknown',
            message: '处理中',
            idempotencyKey: 'k1',
            operationId: 'op1',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolvePostUnknownMutation(),
            { queryClient: client },
        )

        await result.current.mutateAsync('k1')
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useReverseFactMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn and invalidates on a succeeded reversal', async () => {
        mockedApi.reverseFact.mockResolvedValue({
            status: 'succeeded',
            reverseFactId: 'cz1',
            reverseFactNo: 'CZ-1',
            operationId: 'op1',
            message: '已追加回款冲正记录，原回款保留。',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useReverseFactMutation(),
            { queryClient: client },
        )

        const input = {
            kind: 'receipt_reverse' as const,
            sourceFactId: 'r1',
            reason: '纠错',
            idempotencyKey: 'k1',
        }
        await result.current.mutateAsync(input)

        expect(mockedApi.reverseFact).toHaveBeenCalledTimes(1)
        expect(mockedApi.reverseFact.mock.calls[0][0]).toEqual(input)

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customer-receivables'],
        })
    })

    it('does not invalidate when the reversal failed', async () => {
        mockedApi.reverseFact.mockResolvedValue({
            status: 'failed',
            code: 'CUSTOMER_REQUIRED',
            message: '回款未关联经营客户，无法登记退款（后端要求 customer_id）。',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useReverseFactMutation(),
            { queryClient: client },
        )

        await result.current.mutateAsync({
            kind: 'refund',
            sourceFactId: 'r1',
            reason: '退',
            idempotencyKey: 'k2',
        })
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})
