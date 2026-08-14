import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import * as publicationsApi from '@/features/product-publications/api/publications'
import {
    useManualPauseMutation,
    usePublicationDetailQuery,
    usePublicationListQuery,
    usePublishRevisionMutation,
    useRetryDeliveryMutation,
} from '@/features/product-publications/hooks/queries'
import type {
    ManualPauseCommand,
    ManualPauseResult,
    ProductPublicationListQuery,
    ProductPublicationListResult,
    ProductPublicationView,
    PublishRevisionCommand,
    PublishRevisionResult,
    RetryDeliveryCommand,
    RetryDeliveryResult,
} from '@/features/product-publications/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/product-publications/api/publications', () => ({
    fetchPublicationDetail: vi.fn(),
    fetchPublicationList: vi.fn(),
    manualPausePublication: vi.fn(),
    publishRevision: vi.fn(),
    retryDelivery: vi.fn(),
}))

const mockedApi = vi.mocked(publicationsApi)

const baseQuery: ProductPublicationListQuery = {
    publicationStatus: 'all',
    deliveryStatus: 'all',
    metric: 'all',
    page: 1,
    pageSize: 20,
}

const listResult = (): ProductPublicationListResult => ({
    items: [],
    total: 0,
    page: 1,
    pageSize: 20,
    metrics: {
        pendingPublish: 0,
        pendingConfirm: 0,
        failedOrHandoff: 0,
        mallLive: 0,
        paused: 0,
    },
    permissionVersion: 'pv-live',
    dataScopeVersion: 'ds-live',
    queriedAt: '2026-01-01T00:00:00.000Z',
    creationBlocker: {
        code: 'PUBLICATION_IDENTITY_POLICY_UNCONFIRMED',
        message: '新建发布身份策略尚未确认。',
    },
    filterSummary: '0 条',
    resolvedFilters: {},
})

const detailView = (): ProductPublicationView =>
    ({
        identity: {
            publicationId: 'pub-1',
            publicationCode: 'PUB-1',
            skuId: 'sku-1',
            skuCode: 'SKU-001',
            targetMallId: 'mall-1',
            targetMallName: '测试商城',
        },
        status: 'MALL_LIVE',
        statusLabel: '商城已生效',
        statusTone: 'success',
        selectedRevision: { name: '测试商品' },
    }) as unknown as ProductPublicationView

describe('usePublicationListQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the list with the given query under a stable key', async () => {
        mockedApi.fetchPublicationList.mockResolvedValue(listResult())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => usePublicationListQuery(baseQuery),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)

        await waitFor(() => expect(result.current.data).toEqual(listResult()))
        expect(mockedApi.fetchPublicationList).toHaveBeenCalledWith(baseQuery)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['product-publications', 'list', baseQuery],
        ])
    })

    it('rebuilds the query key from the params (key stability on same shape)', async () => {
        mockedApi.fetchPublicationList.mockResolvedValue(listResult())

        const client = createFreshQueryClient()
        const { result, rerender } = renderHookWithProviders(
            () => usePublicationListQuery(baseQuery),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))

        const sameShape = { ...baseQuery }
        rerender()
        expect(client.getQueryCache().getAll()).toHaveLength(1)
        expect(client.getQueryCache().getAll()[0]?.queryKey).toEqual([
            'product-publications',
            'list',
            sameShape,
        ])
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchPublicationList.mockRejectedValue(new Error('boom'))

        const { result } = renderHookWithProviders(() =>
            usePublicationListQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('usePublicationDetailQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the detail for the given id under the detail key', async () => {
        mockedApi.fetchPublicationDetail.mockResolvedValue(detailView())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => usePublicationDetailQuery('pub-1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(detailView()))
        expect(mockedApi.fetchPublicationDetail).toHaveBeenCalledWith(
            'pub-1',
            undefined,
        )
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['product-publications', 'detail', 'pub-1', 'latest'],
        ])
    })

    it('includes the revisionId in both the key and the queryFn args', async () => {
        mockedApi.fetchPublicationDetail.mockResolvedValue(detailView())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => usePublicationDetailQuery('pub-1', 'rev-2'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchPublicationDetail).toHaveBeenCalledWith(
            'pub-1',
            'rev-2',
        )
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['product-publications', 'detail', 'pub-1', 'rev-2'],
        ])
    })

    it('stays disabled and never fetches when publicationId is null', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => usePublicationDetailQuery(null),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchPublicationDetail).not.toHaveBeenCalled()
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchPublicationDetail.mockRejectedValue(new Error('down'))

        const { result } = renderHookWithProviders(() =>
            usePublicationDetailQuery('pub-1'),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

const publishCommand: PublishRevisionCommand = {
    publicationId: 'pub-1',
    expectedObjectVersion: '1',
    expectedPublishGateVersion: '1',
    requestId: 'req-1',
    content: {
        skuRevisionId: 'sku-rev-1',
        supplierOfferingRevisionId: 'offer-1',
        categoryId: 'c1',
        name: '测试商品',
        specification: '规格',
        salesDescription: '说明',
        minimumPurchaseQuantity: '1',
        salesPriceGross: '9.90',
        salesTaxRate: '0.13',
        baseUnitCode: 'PCS',
        salesRegion: [],
        saleStatus: 'ON_SALE',
        productCapabilities: [],
        validFrom: '2026-01-01T00:00:00.000Z',
        media: [],
    },
}

const publishSucceeded = (): PublishRevisionResult => ({
    status: 'succeeded',
    operationId: 'op-1',
    publicationId: 'pub-1',
    revisionId: 'rev-1',
    revisionNo: 2,
    deliveryId: 'del-1',
    deliveryStatus: 'PENDING_SEND',
    committedAt: '2026-01-01T00:00:00.000Z',
})

describe('usePublishRevisionMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to publishRevision and invalidates all publication queries on success', async () => {
        mockedApi.publishRevision.mockResolvedValue(publishSucceeded())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => usePublishRevisionMutation(),
            { queryClient: client },
        )

        let value: PublishRevisionResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(publishCommand)
        })

        expect(mockedApi.publishRevision).toHaveBeenCalledTimes(1)
        expect(mockedApi.publishRevision.mock.calls[0]?.[0]).toEqual(
            publishCommand,
        )
        expect(value).toEqual(publishSucceeded())
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['product-publications'],
        })
    })

    it('skips invalidation when the result is blocked', async () => {
        const blocked: PublishRevisionResult = {
            status: 'blocked',
            code: 'REVIEW_BLOCKED',
            message: '需要复核',
        }
        mockedApi.publishRevision.mockResolvedValue(blocked)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => usePublishRevisionMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(publishCommand)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.publishRevision.mockRejectedValue(new Error('fail'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => usePublishRevisionMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(publishCommand).catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

const pauseCommand: ManualPauseCommand = {
    publicationId: 'pub-1',
    expectedObjectVersion: '1',
    requestId: 'req-1',
    reason: '供应商停供',
}

const pauseSucceeded = (): ManualPauseResult => ({
    status: 'succeeded',
    revisionId: '',
    revisionNo: 0,
    deliveryId: '',
    committedAt: '2026-01-01T00:00:00.000Z',
})

describe('useManualPauseMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to manualPausePublication and invalidates on success', async () => {
        mockedApi.manualPausePublication.mockResolvedValue(pauseSucceeded())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useManualPauseMutation(),
            { queryClient: client },
        )

        let value: ManualPauseResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(pauseCommand)
        })

        expect(mockedApi.manualPausePublication).toHaveBeenCalledTimes(1)
        expect(mockedApi.manualPausePublication.mock.calls[0]?.[0]).toEqual(
            pauseCommand,
        )
        expect(value).toEqual(pauseSucceeded())
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['product-publications'],
        })
    })

    it('skips invalidation when the result is unknown', async () => {
        const unknown: ManualPauseResult = {
            status: 'unknown',
            requestId: 'req-1',
            message: '处理结果待确认，请勿重复提交',
        }
        mockedApi.manualPausePublication.mockResolvedValue(unknown)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useManualPauseMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(pauseCommand)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

const retryCommand: RetryDeliveryCommand = {
    publicationId: 'pub-1',
    deliveryId: 'del-1',
    requestId: 'req-1',
}

const retrySucceeded = (): RetryDeliveryResult => ({
    status: 'succeeded',
    deliveryId: 'del-1',
    attemptCount: 1,
    deliveryStatus: 'PENDING_SEND',
})

describe('useRetryDeliveryMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to retryDelivery and invalidates on success', async () => {
        mockedApi.retryDelivery.mockResolvedValue(retrySucceeded())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useRetryDeliveryMutation(),
            { queryClient: client },
        )

        let value: RetryDeliveryResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(retryCommand)
        })

        expect(mockedApi.retryDelivery).toHaveBeenCalledTimes(1)
        expect(mockedApi.retryDelivery.mock.calls[0]?.[0]).toEqual(
            retryCommand,
        )
        expect(value).toEqual(retrySucceeded())
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['product-publications'],
        })
    })

    it('skips invalidation when the result is blocked', async () => {
        const blocked: RetryDeliveryResult = {
            status: 'blocked',
            code: 'NO_REVISION',
            message: '无可重试的发布修订',
        }
        mockedApi.retryDelivery.mockResolvedValue(blocked)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useRetryDeliveryMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(retryCommand)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})
