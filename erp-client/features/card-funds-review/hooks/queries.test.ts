import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, cleanup, waitFor } from '@testing-library/react'

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'
import type {
    CardFundsReviewQueueView,
    FormalActionResponse,
    RegisterFundsResult,
} from '@/features/card-funds-review/types'
import {
    useCardFundsReviewQueueQuery,
    useCompleteCardFundsMutation,
    useRegisterInvoiceMutation,
    useRegisterReceiptMutation,
} from './queries'
import {
    completeCardFundsReview,
    fetchCardFundsReviewQueue,
    registerHistoricalInvoice,
    registerHistoricalReceipt,
} from '@/features/card-funds-review/api'
import {
    makeCompleteCommand,
    makeQueueQuery,
    makeQueueView,
    makeTask,
} from './test-data'

vi.mock('@/features/card-funds-review/api', () => ({
    fetchCardFundsReviewQueue: vi.fn(),
    completeCardFundsReview: vi.fn(),
    registerHistoricalReceipt: vi.fn(),
    registerHistoricalInvoice: vi.fn(),
}))

const mockedFetchQueue = vi.mocked(fetchCardFundsReviewQueue)
const mockedComplete = vi.mocked(completeCardFundsReview)
const mockedRegisterReceipt = vi.mocked(registerHistoricalReceipt)
const mockedRegisterInvoice = vi.mocked(registerHistoricalInvoice)

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe('useCardFundsReviewQueueQuery', () => {
    it('passes the query to the queryFn and exposes data after load', async () => {
        const query = makeQueueQuery({ type: 'opening' })
        const view: CardFundsReviewQueueView = makeQueueView(makeTask())
        mockedFetchQueue.mockResolvedValue(view)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCardFundsReviewQueueQuery(query),
            { queryClient: client },
        )
        expect(result.current.isPending).toBe(true)

        await waitFor(() => expect(result.current.isPending).toBe(false))
        expect(result.current.data).toEqual(view)
        expect(mockedFetchQueue).toHaveBeenCalledWith(query)
        expect(mockedFetchQueue).toHaveBeenCalledTimes(1)
    })

    it('reuses the cache for a structurally equal query key', async () => {
        const query = makeQueueQuery()
        mockedFetchQueue.mockResolvedValue(makeQueueView(makeTask()))

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCardFundsReviewQueueQuery(query),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        // 同一 QueryClient 下等价参数共享同一条缓存（queryKey 稳定）
        const second = renderHookWithProviders(
            () => useCardFundsReviewQueueQuery({ ...query }),
            { queryClient: client },
        )
        await waitFor(() => expect(second.result.current.data).toBeDefined())
        const cacheEntries = client.getQueryCache().findAll()
        expect(cacheEntries).toHaveLength(1)
        expect(cacheEntries[0]?.queryKey).toEqual([
            'card-funds-review',
            'queue',
            query,
        ])
    })

    it('refetches when the query params change', async () => {
        const query = makeQueueQuery({ type: 'opening' })
        mockedFetchQueue.mockResolvedValue(makeQueueView(makeTask()))

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCardFundsReviewQueueQuery(query),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isPending).toBe(false))

        const second = renderHookWithProviders(
            () => useCardFundsReviewQueueQuery(makeQueueQuery({ type: 'delta' })),
            { queryClient: client },
        )
        await waitFor(() => expect(second.result.current.isPending).toBe(false))
        expect(mockedFetchQueue).toHaveBeenCalledTimes(2)
    })

    it('exposes the error state when the queryFn rejects', async () => {
        mockedFetchQueue.mockRejectedValue(new Error('加载失败'))

        const { result } = renderHookWithProviders(() =>
            useCardFundsReviewQueueQuery(makeQueueQuery()),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
        expect(result.current.data).toBeUndefined()
    })
})

describe('useCompleteCardFundsMutation', () => {
    it('wires the api call and invalidates the queue on success', async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const command = makeCompleteCommand()
        mockedComplete.mockResolvedValue({
            status: 'succeeded',
            outcome: {
                kind: 'APPROVED',
                business: {
                    receivableFundsReviewId: 'rfr_1',
                    receivableAccountId: 'acct_1',
                    reviewNo: 7,
                    accountReviewStatus: 'reviewed',
                    workflowActionId: 'wa_1',
                    operationId: 'op_1',
                    completedAt: '2026-07-01T08:00:00.000Z',
                    reviewResult: 'APPROVED',
                    conclusion: 'RECORDED_FACTS_RECONCILED',
                },
            },
        } satisfies FormalActionResponse)

        const { result } = renderHookWithProviders(
            () => useCompleteCardFundsMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(command)
        })
        expect(mockedComplete.mock.calls[0]?.[0]).toEqual(command)
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['card-funds-review'],
            }),
        )
    })

    it('does not invalidate the queue when the result is not succeeded', async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        mockedComplete.mockResolvedValue({
            status: 'failed',
            code: 'SUBJECT_HASH_MISMATCH',
            message: '数据版本不匹配',
        } satisfies FormalActionResponse)

        const { result } = renderHookWithProviders(
            () => useCompleteCardFundsMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(makeCompleteCommand())
        })
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useRegisterReceiptMutation', () => {
    it('calls the receipt api and invalidates the queue on success', async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const input = {
            workItemId: 'wi_1',
            expectedSubjectVersion: 'sv_1',
            receiptNo: 'SK-001',
            receivedAt: '2026-07-01',
            grossAmount: '100.00',
            allocations: [],
            evidenceReference: '银行回单',
        }
        const payload: RegisterFundsResult = {
            fundsFactVersion: 'ffv_2',
            subjectHash: 'sh_2',
            settledTotal: '100.00',
            invoicedTotal: '0.00',
            openTotal: '1030.00',
            openInvoiceableTotal: '1130.00',
            receiptFacts: [],
            invoiceFacts: [],
        }
        mockedRegisterReceipt.mockResolvedValue(payload)

        const { result } = renderHookWithProviders(
            () => useRegisterReceiptMutation(),
            { queryClient: client },
        )
        let output: RegisterFundsResult | undefined
        await act(async () => {
            output = await result.current.mutateAsync(input)
        })
        expect(output).toEqual(payload)
        expect(mockedRegisterReceipt.mock.calls[0]?.[0]).toEqual(input)
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['card-funds-review'],
            }),
        )
    })
})

describe('useRegisterInvoiceMutation', () => {
    it('calls the invoice api and invalidates the queue on success', async () => {
        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const input = {
            workItemId: 'wi_1',
            expectedSubjectVersion: 'sv_1',
            invoiceNo: 'FP-001',
            issuedAt: '2026-07-01',
            grossAmount: '113.00',
            netAmount: '100.00',
            taxAmount: '13.00',
            allocations: [],
            evidenceReference: '发票扫描件',
        }
        mockedRegisterInvoice.mockResolvedValue({
            fundsFactVersion: 'ffv_2',
            subjectHash: 'sh_2',
            settledTotal: '0.00',
            invoicedTotal: '113.00',
            openTotal: '1130.00',
            openInvoiceableTotal: '1017.00',
            receiptFacts: [],
            invoiceFacts: [],
        })

        const { result } = renderHookWithProviders(
            () => useRegisterInvoiceMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(mockedRegisterInvoice.mock.calls[0]?.[0]).toEqual(input)
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['card-funds-review'],
            }),
        )
    })
})
