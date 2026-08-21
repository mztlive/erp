import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import * as masterDataApi from '@/features/master-data/api'
import {
    masterDataKeys,
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useDisableMasterDataMutation,
    useMasterDataCenterQuery,
    useMasterDataExportMutation,
    useMasterDataListQuery,
    useProductFilterOptionsQuery,
    useProductListingMutation,
    useProductListSkusQuery,
    useSkuSupplierCountsQuery,
} from '@/features/master-data/hooks/queries'
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataCenterView,
    MasterDataListQuery,
    MasterDataListResult,
    MasterDataMutationResult,
    MasterDataResource,
} from '@/features/master-data/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/master-data/api', () => ({
    createMasterDataObject: vi.fn(),
    createMasterDataRevision: vi.fn(),
    disableMasterDataObject: vi.fn(),
    fetchMasterDataCenter: vi.fn(),
    fetchMasterDataList: vi.fn(),
    fetchProductFilterOptions: vi.fn(),
    fetchProductListSkus: vi.fn(),
    fetchSkuSupplierCounts: vi.fn(),
    updateProductListingStatus: vi.fn(),
}))

const mockedApi = vi.mocked(masterDataApi)

const listQuery: MasterDataListQuery = { resource: 'categories' }

const listResult = (): MasterDataListResult => ({
    resource: 'categories',
    rows: [],
    totalCount: 0,
    permissionVersion: 'pv-w14-http-1',
    effectiveAsOf: '2026-01-01T00:00:00.000Z',
    eligibilityAsOf: '2026-01-01T00:00:00.000Z',
    queriedAt: '2026-01-01T00:00:00.000Z',
    metrics: [],
})

const succeeded = (): MasterDataMutationResult => ({
    outcome: 'succeeded',
    stableId: 'c1',
    stableNo: 'C-1',
    revisionId: 'r1',
    revisionNo: 1,
    revisionState: 'CURRENT',
    effectiveFrom: '2026-01-01',
    recordedAt: '2026-01-01T00:00:00.000Z',
    actor: '系统',
    changeReason: '新建',
    reference: 'MD-CREATE-C-1',
    nextActions: ['查看详情'],
})

const createInput: CreateMasterDataInput = {
    resource: 'categories',
    name: '测试分类',
    effectiveFrom: '2026-01-01',
    changeReason: '新建',
    fields: { code: 'C1' },
    idempotencyKey: 'create-c1',
}

const revisionInput: CreateRevisionInput = {
    resource: 'categories',
    stableId: 'c1',
    baseRevisionId: 'r1',
    expectedLockVersion: 1,
    name: '测试分类',
    effectiveFrom: '2026-01-02',
    changeReason: '更新',
    fields: { code: 'C1' },
    idempotencyKey: 'rev-c1',
}

const disableInput: DisableMasterDataInput = {
    resource: 'categories',
    stableId: 'c1',
    baseRevisionId: 'r1',
    expectedLockVersion: 1,
    changeReason: '停用',
    effectiveFrom: '2026-01-02',
    idempotencyKey: 'dis-c1',
}

const invalidatedKeys = () => ({
    masterData: { queryKey: ['master-data'] },
    companySkus: { queryKey: ['supplier-offerings', 'company-skus'] },
    units: { queryKey: ['options', 'units'] },
})

describe('masterDataKeys', () => {
    it('builds the list key with the query object', () => {
        expect(masterDataKeys.list(listQuery)).toEqual([
            'master-data',
            'list',
            listQuery,
        ])
    })

    it('builds the detail key with resource and stable id', () => {
        expect(masterDataKeys.detail('products', 'p1')).toEqual([
            'master-data',
            'detail',
            'products',
            'p1',
        ])
    })
})

describe('useMasterDataListQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the list under a stable key', async () => {
        mockedApi.fetchMasterDataList.mockResolvedValue(listResult())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useMasterDataListQuery(listQuery),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.data).toEqual(listResult()),
        )
        expect(mockedApi.fetchMasterDataList).toHaveBeenCalledWith(listQuery)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['master-data', 'list', listQuery],
        ])
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchMasterDataList.mockRejectedValue(new Error('down'))

        const { result } = renderHookWithProviders(() =>
            useMasterDataListQuery(listQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })

    it('keeps the previous list while a new filter query is in flight', async () => {
        const first = { ...listResult(), totalCount: 3 }
        const second = { ...listResult(), totalCount: 1 }
        let resolveNext: ((value: MasterDataListResult) => void) | undefined
        mockedApi.fetchMasterDataList
            .mockResolvedValueOnce(first)
            .mockImplementationOnce(
                () =>
                    new Promise<MasterDataListResult>((resolve) => {
                        resolveNext = resolve
                    }),
            )

        const query = { current: { ...listQuery } }
        const { result, rerender } = renderHookWithProviders(() =>
            useMasterDataListQuery(query.current),
        )

        await waitFor(() => expect(result.current.data).toEqual(first))

        query.current = { ...listQuery, q: '笔' }
        rerender()

        await waitFor(() => expect(result.current.isFetching).toBe(true))
        expect(result.current.isPending).toBe(false)
        expect(result.current.isPlaceholderData).toBe(true)
        expect(result.current.data).toEqual(first)

        resolveNext?.(second)
        await waitFor(() => expect(result.current.data).toEqual(second))
        expect(result.current.isPlaceholderData).toBe(false)
        expect(result.current.isPending).toBe(false)
    })
})

describe('useProductListSkusQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('dedupes, drops empties and sorts the product ids', async () => {
        mockedApi.fetchProductListSkus.mockResolvedValue([])

        const { result } = renderHookWithProviders(() =>
            useProductListSkusQuery(['b', 'a', 'b', '']),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchProductListSkus).toHaveBeenCalledWith(['a', 'b'])
    })

    it('stays disabled and never fetches when there are no ids', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProductListSkusQuery([]),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchProductListSkus).not.toHaveBeenCalled()
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchProductListSkus.mockRejectedValue(new Error('boom'))

        const { result } = renderHookWithProviders(() =>
            useProductListSkusQuery(['p1']),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useProductFilterOptionsQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled and never fetches while enabled is false', () => {
        const { result } = renderHookWithProviders(() =>
            useProductFilterOptionsQuery(false),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchProductFilterOptions).not.toHaveBeenCalled()
    })

    it('fetches filter options once enabled', async () => {
        const options = { categories: [], brands: [], suppliers: [] }
        mockedApi.fetchProductFilterOptions.mockResolvedValue(options)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useProductFilterOptionsQuery(true),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(options))
        expect(mockedApi.fetchProductFilterOptions).toHaveBeenCalledTimes(1)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['master-data', 'product-filter-options'],
        ])
    })
})

describe('useMasterDataExportMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('re-queries the server with the same list query', async () => {
        mockedApi.fetchMasterDataList.mockResolvedValue(listResult())

        const { result } = renderHookWithProviders(() =>
            useMasterDataExportMutation(),
        )

        let value: MasterDataListResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(listQuery)
        })

        expect(mockedApi.fetchMasterDataList).toHaveBeenCalledWith(listQuery)
        expect(value).toEqual(listResult())
    })
})

describe('useMasterDataCenterQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('stays disabled and never fetches for an empty stable id', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useMasterDataCenterQuery('products', ''),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchMasterDataCenter).not.toHaveBeenCalled()
    })

    it('fetches the center under the detail key', async () => {
        const resource: MasterDataResource = 'products'
        const center = { resource, stableId: 'p1' } as MasterDataCenterView
        mockedApi.fetchMasterDataCenter.mockResolvedValue(center)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useMasterDataCenterQuery(resource, 'p1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(center))
        expect(mockedApi.fetchMasterDataCenter).toHaveBeenCalledWith(
            resource,
            'p1',
        )
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['master-data', 'detail', 'products', 'p1'],
        ])
    })

    it('surfaces null data when the api returns null', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            useMasterDataCenterQuery('categories', 'missing'),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchMasterDataCenter.mockRejectedValue(new Error('down'))

        const { result } = renderHookWithProviders(() =>
            useMasterDataCenterQuery('products', 'p1'),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useSkuSupplierCountsQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('dedupes, drops empties and sorts the sku ids', async () => {
        mockedApi.fetchSkuSupplierCounts.mockResolvedValue(new Map())

        const { result } = renderHookWithProviders(() =>
            useSkuSupplierCountsQuery(['s2', 's1', 's2', '']),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchSkuSupplierCounts).toHaveBeenCalledWith([
            's1',
            's2',
        ])
    })

    it('stays disabled and never fetches when there are no ids', () => {
        const { result } = renderHookWithProviders(() =>
            useSkuSupplierCountsQuery([]),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchSkuSupplierCounts).not.toHaveBeenCalled()
    })
})

describe('useCreateMasterDataMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('creates via the api and invalidates related caches on success', async () => {
        mockedApi.createMasterDataObject.mockResolvedValue(succeeded())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateMasterDataMutation(),
            { queryClient: client },
        )

        let value: MasterDataMutationResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(createInput)
        })

        expect(mockedApi.createMasterDataObject).toHaveBeenCalledWith(
            createInput,
        )
        expect(value).toEqual(succeeded())
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(3))
        expect(invalidateSpy).toHaveBeenCalledWith(
            invalidatedKeys().masterData,
        )
        expect(invalidateSpy).toHaveBeenCalledWith(
            invalidatedKeys().companySkus,
        )
        expect(invalidateSpy).toHaveBeenCalledWith(invalidatedKeys().units)
    })

    it('skips invalidation when the outcome is blocked', async () => {
        mockedApi.createMasterDataObject.mockResolvedValue({
            outcome: 'blocked',
            code: 'VALIDATION',
            message: '请求未通过业务校验',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateMasterDataMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(createInput)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.createMasterDataObject.mockRejectedValue(new Error('fail'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateMasterDataMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(createInput).catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useCreateRevisionMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('invalidates related caches on success', async () => {
        mockedApi.createMasterDataRevision.mockResolvedValue(succeeded())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateRevisionMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(revisionInput)
        })

        expect(mockedApi.createMasterDataRevision).toHaveBeenCalledWith(
            revisionInput,
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(3))
    })

    it('invalidates related caches on conflict so the lock version refreshes', async () => {
        mockedApi.createMasterDataRevision.mockResolvedValue({
            outcome: 'conflict',
            message: '资料已被他人更新，请刷新后重新填写。',
            serverLockVersion: 2,
            serverRevisionNo: 2,
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateRevisionMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(revisionInput)
        })

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(3))
    })

    it('skips invalidation when the outcome is blocked', async () => {
        mockedApi.createMasterDataRevision.mockResolvedValue({
            outcome: 'blocked',
            code: 'VALIDATION',
            message: '请求未通过业务校验',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateRevisionMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(revisionInput)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useDisableMasterDataMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('invalidates related caches on success', async () => {
        mockedApi.disableMasterDataObject.mockResolvedValue(succeeded())

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useDisableMasterDataMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(disableInput)
        })

        expect(mockedApi.disableMasterDataObject).toHaveBeenCalledWith(
            disableInput,
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(3))
    })

    it('skips invalidation when the outcome is blocked', async () => {
        mockedApi.disableMasterDataObject.mockResolvedValue({
            outcome: 'blocked',
            code: 'VALIDATION',
            message: '请求未通过业务校验',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useDisableMasterDataMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(disableInput)
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useProductListingMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('updates the listing status via the api and invalidates on success', async () => {
        mockedApi.updateProductListingStatus.mockResolvedValue({
            product_id: 'p1',
            listing_status: 'listed',
            listed_sku_count: 1,
            sku_count: 1,
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useProductListingMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({
                productId: 'p1',
                listingStatus: 'LISTED',
            })
        })

        expect(mockedApi.updateProductListingStatus).toHaveBeenCalledWith(
            'p1',
            'LISTED',
        )
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(3))
    })
})
