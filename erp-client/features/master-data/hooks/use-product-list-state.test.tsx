import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import { useProductListState } from './use-product-list-state'
import type {
    MasterDataListItem,
    MasterDataListResult,
    ProductListSkuSummary,
} from '@/features/master-data/types'
import type { SupplierOfferingView } from '@/features/supplier-offerings/types'
import { createFreshQueryClient, renderHookWithProviders } from '@/features/test-utils'

const stateMocks = vi.hoisted(() => ({
    listData: null as MasterDataListResult | null,
    listPending: false,
    listError: null as unknown,
    filterOptions: {
        categories: [] as { categoryId: string; categoryCode: string; categoryName: string; parentId?: string }[],
        brands: [] as { value: string; label: string; keywords: string }[],
        suppliers: [] as { value: string; label: string; keywords: string }[],
    },
    filterOptionsPending: false,
    skus: [] as ProductListSkuSummary[],
    skusPending: false,
    skusError: null as unknown,
    offerings: [] as SupplierOfferingView[],
    offeringsPending: false,
    offeringsError: null as unknown,
    listingMutateAsync: vi.fn(),
    exportMutateAsync: vi.fn(),
    accountPermissions: ['product:update'] as readonly string[],
}))

vi.mock('next/navigation', () => ({
    useRouter: () => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() }),
    usePathname: () => '/master-data/products',
    useSearchParams: () => new URLSearchParams(),
    useParams: () => ({}),
}))

vi.mock('@/features/auth/queries', () => ({
    useAccountProfileQuery: () => ({
        data: { permissions: stateMocks.accountPermissions },
        isPending: false,
        isError: false,
        error: null,
    }),
}))

vi.mock('@/features/master-data/hooks/queries', () => ({
    useMasterDataListQuery: () => ({
        data: stateMocks.listData,
        isPending: stateMocks.listPending,
        isError: stateMocks.listError != null,
        error: stateMocks.listError,
        refetch: vi.fn(),
    }),
    useProductFilterOptionsQuery: () => ({
        data: stateMocks.filterOptions,
        isPending: stateMocks.filterOptionsPending,
        isError: false,
        error: null,
    }),
    useProductListSkusQuery: () => ({
        data: stateMocks.skus,
        isPending: stateMocks.skusPending,
        isError: stateMocks.skusError != null,
        error: stateMocks.skusError,
        refetch: vi.fn(),
    }),
    useProductListingMutation: () => ({
        mutateAsync: stateMocks.listingMutateAsync,
        isPending: false,
        variables: undefined,
    }),
    useMasterDataExportMutation: () => ({
        mutateAsync: stateMocks.exportMutateAsync,
    }),
}))

vi.mock('@/features/supplier-offerings/queries', () => ({
    useSupplierOfferingsForSkusQuery: () => ({
        data: stateMocks.offerings,
        isPending: stateMocks.offeringsPending,
        isError: stateMocks.offeringsError != null,
        error: stateMocks.offeringsError,
    }),
}))

function makeRow(overrides: Partial<MasterDataListItem> = {}): MasterDataListItem {
    return {
        objectType: 'products',
        stableId: 'p1',
        stableNo: 'P-001',
        name: '示例商品',
        lifecycleStatus: 'ENABLED',
        lifecycleStatusLabel: '启用',
        lifecycleTone: 'success',
        revisionTiming: 'CURRENT',
        revisionTimingLabel: '当前生效',
        currentRevisionId: 'r1',
        displayedRevisionId: 'r1',
        revisionNo: 1,
        effectiveFrom: '2026-01-01',
        keyFacts: [],
        selectorEligibility: [],
        allowedActions: [],
        actionBlockers: [],
        lockVersion: 1,
        metricTags: [],
        ...overrides,
    }
}

function makeListResult(rows: readonly MasterDataListItem[]): MasterDataListResult {
    return {
        resource: 'products',
        rows,
        totalCount: rows.length,
        permissionVersion: 'v1',
        effectiveAsOf: '2026-08-14',
        eligibilityAsOf: '2026-08-14',
        queriedAt: '2026-08-14T00:00:00.000Z',
        metrics: [],
    }
}

function renderState() {
    const searchInputRef = { current: null as HTMLInputElement | null }
    return renderHookWithProviders(() => useProductListState(searchInputRef), {
        queryClient: createFreshQueryClient(),
    })
}

describe('useProductListState', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        stateMocks.listData = null
        stateMocks.listPending = false
        stateMocks.listError = null
        stateMocks.skus = []
        stateMocks.skusPending = false
        stateMocks.skusError = null
        stateMocks.offerings = []
        stateMocks.offeringsPending = false
        stateMocks.offeringsError = null
        stateMocks.accountPermissions = ['product:update']
        URL.createObjectURL = vi.fn(() => 'blob:csv')
        URL.revokeObjectURL = vi.fn()
    })

    afterEach(() => {
        vi.restoreAllMocks()
    })

    it('slices server rows into the current client page', () => {
        const rows = Array.from({ length: 25 }, (_, i) =>
            makeRow({ stableId: `p${i}`, stableNo: `P-${i}` }),
        )
        stateMocks.listData = makeListResult(rows)
        const { result } = renderState()

        expect(result.current.rows).toHaveLength(25)
        expect(result.current.pageRows).toHaveLength(20)
        expect(result.current.pageRows[0].stableId).toBe('p0')
        expect(result.current.pageRows[19].stableId).toBe('p19')
    })

    it('groups page skus by product id', () => {
        stateMocks.skus = [
            { productId: 'p1', skuId: 'sk1', skuNo: 'SKU-01', skuName: '红', specification: '颜色：红', baseUnit: '件' },
            { productId: 'p1', skuId: 'sk2', skuNo: 'SKU-02', skuName: '蓝', specification: '颜色：蓝', baseUnit: '件' },
            { productId: 'p2', skuId: 'sk3', skuNo: 'SKU-03', skuName: '大', specification: '规格：大', baseUnit: '件' },
        ]
        const { result } = renderState()

        expect(result.current.productSkusByProduct.get('p1')).toHaveLength(2)
        expect(result.current.productSkusByProduct.get('p2')).toHaveLength(1)
        expect(result.current.productPageSkuIds).toEqual(['sk1', 'sk2', 'sk3'])
    })

    it('keeps only active offerings with a current revision as supplied skus', () => {
        stateMocks.offerings = [
            { sku_id: 'sk1', status: 'ACTIVE', current_revision_id: 'rev1' },
            { sku_id: 'sk2', status: 'ACTIVE', current_revision_id: null },
            { sku_id: 'sk3', status: 'PAUSED', current_revision_id: 'rev2' },
        ] as unknown as SupplierOfferingView[]
        const { result } = renderState()

        expect(result.current.currentSupplySkuIds).toEqual(new Set(['sk1']))
    })

    it('derives create and listing permissions from the account', () => {
        const { result: withoutCreate } = renderState()
        expect(withoutCreate.current.canCreate).toBe(false)
        expect(withoutCreate.current.canUpdateProductListing).toBe(true)

        stateMocks.accountPermissions = ['product:create']
        const { result: withCreate } = renderState()
        expect(withCreate.current.canCreate).toBe(true)
        expect(withCreate.current.canUpdateProductListing).toBe(false)
    })

    it('updates the listing status after confirmation', async () => {
        const row = makeRow()
        stateMocks.listData = makeListResult([row])
        stateMocks.listingMutateAsync.mockResolvedValue({})
        vi.spyOn(window, 'confirm').mockReturnValue(true)
        const { result } = renderState()

        await act(async () => {
            await result.current.updateProductListing(row, false)
        })

        expect(stateMocks.listingMutateAsync).toHaveBeenCalledWith({
            productId: 'p1',
            listingStatus: 'UNLISTED',
        })
        expect(result.current.listingError).toBeNull()
    })

    it('aborts the listing change when the user declines the confirmation', async () => {
        const row = makeRow()
        vi.spyOn(window, 'confirm').mockReturnValue(false)
        const { result } = renderState()

        await act(async () => {
            await result.current.updateProductListing(row, false)
        })

        expect(stateMocks.listingMutateAsync).not.toHaveBeenCalled()
    })

    it('surfaces listing errors in a readable message', async () => {
        const row = makeRow()
        vi.spyOn(window, 'confirm').mockReturnValue(true)
        stateMocks.listingMutateAsync.mockRejectedValue(
            new Error('网络中断'),
        )
        const { result } = renderState()

        await act(async () => {
            await result.current.updateProductListing(row, true)
        })

        expect(result.current.listingError).toBe('网络中断')
    })

    it('exports nothing without loaded rows', async () => {
        const { result } = renderState()

        await act(async () => {
            await result.current.onExport()
        })

        expect(stateMocks.exportMutateAsync).not.toHaveBeenCalled()
    })

    it('exports the current filters via a fresh server query', async () => {
        const row = makeRow()
        stateMocks.listData = makeListResult([row])
        stateMocks.exportMutateAsync.mockResolvedValue(
            makeListResult([row]),
        )
        const { result } = renderState()

        await act(async () => {
            await result.current.onExport()
        })

        expect(stateMocks.exportMutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({ resource: 'products' }),
        )
        await waitFor(() => expect(result.current.exportMeta).not.toBeNull())
    })
})
