import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import * as masterDataApi from '@/features/master-data/api'
import { useProductEditor } from './use-product-editor'
import type {
    MasterDataCenterView,
    ProductFields,
} from '@/features/master-data/types'
import { createFreshQueryClient, renderHookWithProviders } from '@/features/test-utils'

const authMocks = vi.hoisted(() => ({
    profile: { permissions: ['product:update'] as readonly string[] },
}))

const toastAdd = vi.hoisted(() => vi.fn())

const navMocks = vi.hoisted(() => ({
    replace: vi.fn(),
    push: vi.fn(),
    back: vi.fn(),
}))

vi.mock('next/navigation', () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
    usePathname: () => '/master-data/products',
    useSearchParams: () => new URLSearchParams(),
    useParams: () => ({}),
}))

vi.mock('@/components/ui/toast', () => ({
    toast: { add: toastAdd },
}))

vi.mock('@/features/auth/queries', () => ({
    useAccountProfileQuery: () => ({
        data: authMocks.profile,
        isPending: false,
        isError: false,
        error: null,
    }),
}))

vi.mock('@/hooks/use-options', () => ({
    useUnitOptionsQuery: () => ({ data: undefined }),
    optionKeys: {
        suppliers: ['options', 'suppliers'],
        parties: ['options', 'parties'],
        owners: ['options', 'owners'],
        team: ['options', 'team'],
        units: ['options', 'units'],
    },
}))

vi.mock('@/features/master-data/api', () => ({
    createMasterDataObject: vi.fn(),
    createMasterDataRevision: vi.fn(),
    disableMasterDataObject: vi.fn(),
    fetchMasterDataCenter: vi.fn(),
    fetchMasterDataList: vi.fn(),
    fetchProductFilterOptions: vi.fn(),
    fetchProductListSkus: vi.fn(),
    fetchSkuSupplierCounts: vi.fn(),
    revealMasterDataSensitive: vi.fn(),
    updateProductListingStatus: vi.fn(),
}))

vi.mock('@/features/file-assets/api', () => ({
    uploadFileAssetImage: vi.fn(),
}))

const mockedApi = vi.mocked(masterDataApi)

function makeCenter(
    overrides: Partial<MasterDataCenterView> = {},
): MasterDataCenterView {
    return {
        resource: 'products',
        stableId: 'p1',
        stableNo: 'P-001',
        name: '示例商品',
        lifecycleStatus: 'ENABLED',
        lifecycleStatusLabel: '启用',
        lifecycleTone: 'success',
        revisionTiming: 'CURRENT',
        revisionTimingLabel: '当前生效',
        lockVersion: 3,
        currentRevision: {
            revisionId: 'r1',
            revisionNo: 1,
            name: '示例商品',
            effectiveFrom: '2026-01-01',
            changeReason: '新建',
            actor: '系统',
            fields: [],
        },
        revisionTimeline: [],
        selectorEligibility: [],
        usageSummary: { historicalReferenceCount: 0, note: '' },
        sensitiveFields: [],
        resourceFacts: [],
        allowedActions: ['CREATE_REVISION', 'DISABLE'],
        actionBlockers: [],
        auditEvents: [],
        sections: ['overview', 'versions', 'relations', 'audit'],
        productKind: 'PHYSICAL',
        productConstraints: {
            baseUnit: '件',
            hasFormalReferences: false,
            skuCount: 1,
        },
        productDetail: {
            lifecycleStatus: 'ENABLED',
            productNo: 'P-001',
            description: '一支好笔',
            specification: '',
            baseUnitId: 'u1',
            baseUnitCode: 'pc',
            baseUnit: '件',
            categoryId: 'c1',
            category: '办公用品',
            brandId: 'b1',
            brand: '得力',
            carouselImages: [],
            detailImages: [],
            carouselPreviewUrls: {},
            detailPreviewUrls: {},
            carouselFileAssetIds: {},
            detailFileAssetIds: {},
            specs: [],
            skus: [
                {
                    skuId: 'sk1',
                    skuRevisionId: 'skr1',
                    skuNo: 'SKU-01',
                    name: '红色',
                    attributeValues: [],
                    specLabel: '默认规格',
                    mainImage: 'a.png',
                    mainImagePreviewUrl: 'https://cdn/a.png',
                    mainImageAssetId: 'fa-1',
                    salePrice: '10',
                    marketPrice: '12',
                    baseUnit: '件',
                    listingStatus: 'LISTED',
                    lifecycleStatus: 'ENABLED',
                },
            ],
        },
        ...overrides,
    }
}

function makeListResult() {
    return {
        resource: 'products' as const,
        rows: [],
        totalCount: 0,
        permissionVersion: 'v1',
        effectiveAsOf: '2026-08-14',
        eligibilityAsOf: '2026-08-14',
        queriedAt: '2026-08-14T00:00:00.000Z',
        metrics: [],
    }
}

function makeSucceededResult(
    overrides: Record<string, unknown> = {},
): Awaited<ReturnType<typeof masterDataApi.createMasterDataRevision>> {
    return {
        outcome: 'succeeded',
        stableId: 'p1',
        stableNo: 'P-001',
        revisionId: 'r2',
        revisionNo: 2,
        revisionState: 'CURRENT',
        effectiveFrom: '2026-01-01',
        recordedAt: '2026-08-14T00:00:00.000Z',
        actor: '系统',
        changeReason: '更新',
        reference: 'ref-1',
        nextActions: [],
        ...overrides,
    } as never
}

function renderEditor(stableId: string) {
    return renderHookWithProviders(() => useProductEditor(stableId), {
        queryClient: createFreshQueryClient(),
    })
}

describe('useProductEditor', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        authMocks.profile = { permissions: ['product:update'] }
        mockedApi.fetchMasterDataList.mockResolvedValue(makeListResult())
        mockedApi.fetchSkuSupplierCounts.mockResolvedValue(new Map())
        mockedApi.fetchMasterDataCenter.mockResolvedValue(null)
    })

    it('hydrates the form from the product center once the detail loads', async () => {
        const center = makeCenter()
        mockedApi.fetchMasterDataCenter.mockResolvedValue(center)
        const { result } = renderEditor('p1')

        await waitFor(() => expect(result.current.data).toEqual(center))

        expect(mockedApi.fetchMasterDataCenter).toHaveBeenCalledWith(
            'products',
            'p1',
        )
        expect(result.current.form.state.values.name).toBe('示例商品')
        expect(result.current.form.state.values.effectiveFrom).toBe(
            '2026-01-01',
        )
        expect(result.current.form.state.values.fields.productKind).toBe(
            'PHYSICAL',
        )
        expect(result.current.form.state.values.fields.skus[0].skuNo).toBe(
            'SKU-01',
        )
        expect(result.current.form.state.values.specDrafts).toEqual([])
    })

    it('derives permissions from the account and allowed actions', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(makeCenter())
        const { result } = renderEditor('p1')
        await waitFor(() => expect(result.current.data).toBeTruthy())

        expect(result.current.hasUpdatePermission).toBe(true)
        expect(result.current.canRevise).toBe(true)
        expect(result.current.canDisable).toBe(true)

        authMocks.profile = { permissions: [] }
        const { result: restricted } = renderEditor('p2')
        await waitFor(() => expect(restricted.current.hasUpdatePermission).toBe(false))
        expect(restricted.current.canRevise).toBe(false)
        expect(restricted.current.canDisable).toBe(false)
    })

    it('honours action blockers when revising', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(
            makeCenter({
                allowedActions: ['CREATE_REVISION'],
                actionBlockers: [
                    {
                        action: 'CREATE_REVISION',
                        code: 'B1',
                        message: '存在未完成的导入任务',
                    },
                ],
            }),
        )
        const { result } = renderEditor('p1')
        await waitFor(() => expect(result.current.data).toBeTruthy())

        expect(result.current.canRevise).toBe(true)
        expect(result.current.reviseBlocker?.message).toBe(
            '存在未完成的导入任务',
        )
    })

    it('runs the local check and reports validation failures', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(makeCenter())
        const { result } = renderEditor('p1')
        await waitFor(() => expect(result.current.data).toBeTruthy())

        act(() => {
            result.current.form.setFieldValue('name', 'x')
            result.current.form.setFieldValue('changeReason', '改名')
        })
        act(() => {
            result.current.runLocalCheck(result.current.form.state.values)
        })

        expect(result.current.formError).toBe('请填写商品名称')
        expect(result.current.checkPassed).toBe(false)

        act(() => {
            result.current.form.setFieldValue('name', '示例商品')
        })
        act(() => {
            result.current.runLocalCheck(result.current.form.state.values)
        })

        expect(result.current.formError).toBeNull()
        expect(result.current.checkPassed).toBe(true)
    })

    it('revises the product through the mutation and refreshes the detail', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(makeCenter())
        mockedApi.createMasterDataRevision.mockResolvedValue(
            makeSucceededResult(),
        )
        const { result } = renderEditor('p1')
        await waitFor(() => expect(result.current.data).toBeTruthy())

        act(() => {
            result.current.form.setFieldValue('changeReason', '更新名称')
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })

        expect(mockedApi.createMasterDataRevision).toHaveBeenCalledWith(
            expect.objectContaining({
                resource: 'products',
                stableId: 'p1',
                baseRevisionId: 'r1',
                expectedLockVersion: 3,
                name: '示例商品',
                changeReason: '更新名称',
                fields: expect.objectContaining({
                    productNo: 'P-001',
                    skus: expect.any(Array),
                }),
                idempotencyKey: expect.stringContaining('revise-product-'),
            }),
        )
        expect(toastAdd).toHaveBeenCalledWith(
            expect.objectContaining({ type: 'success' }),
        )
        // 初始加载 + 成功后显式 refetch + 变更失效触发的一次自动重取
        expect(mockedApi.fetchMasterDataCenter).toHaveBeenCalledTimes(3)
    })

    it('surfaces a blocked revision result without refreshing', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(makeCenter())
        mockedApi.createMasterDataRevision.mockResolvedValue({
            outcome: 'blocked',
            code: 'LOCK',
            message: '数据已更新，请刷新后重试。',
        })
        const { result } = renderEditor('p1')
        await waitFor(() => expect(result.current.data).toBeTruthy())

        act(() => {
            result.current.form.setFieldValue('changeReason', '更新名称')
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })

        expect(result.current.result?.outcome).toBe('blocked')
        expect(toastAdd).not.toHaveBeenCalled()
        expect(mockedApi.fetchMasterDataCenter).toHaveBeenCalledTimes(1)
    })

    it('creates a product and routes to its detail page on success', async () => {
        authMocks.profile = { permissions: ['product:create'] }
        mockedApi.createMasterDataObject.mockResolvedValue(
            makeSucceededResult({ stableId: 'p9', stableNo: 'P-009' }),
        )
        const fields: ProductFields = {
            lifecycleStatus: 'ENABLED',
            productNo: 'P-009',
            description: '',
            specification: '',
            baseUnitId: 'u1',
            baseUnitCode: 'pc',
            baseUnit: '件',
            categoryId: 'c1',
            category: '办公用品',
            brandId: 'b1',
            brand: '得力',
            productKind: 'PHYSICAL',
            carouselImages: [],
            detailImages: [],
            carouselPreviewUrls: {},
            detailPreviewUrls: {},
            carouselFileAssetIds: {},
            detailFileAssetIds: {},
            specs: [],
            skus: [
                {
                    skuId: 'sk9',
                    skuNo: 'SKU-01',
                    name: '红色',
                    attributeValues: [],
                    specLabel: '默认规格',
                    mainImage: 'a.png',
                    mainImagePreviewUrl: 'https://cdn/a.png',
                    mainImageAssetId: 'fa-1',
                    baseUnit: '件',
                    listingStatus: 'UNLISTED',
                    lifecycleStatus: 'ENABLED',
                },
            ],
        }
        const { result } = renderEditor('new')

        expect(result.current.canCreate).toBe(true)
        expect(result.current.form.state.values.name).toBe('')
        expect(result.current.form.state.values.changeReason).toBe('新建商品')

        act(() => {
            result.current.form.setFieldValue('name', '新品签字笔')
            result.current.form.setFieldValue('fields', fields)
        })
        await act(async () => {
            await result.current.form.handleSubmit()
        })

        expect(mockedApi.createMasterDataObject).toHaveBeenCalledWith(
            expect.objectContaining({
                resource: 'products',
                name: '新品签字笔',
                fields: expect.objectContaining({ productNo: 'P-009' }),
                idempotencyKey: expect.stringContaining('create-product-'),
            }),
        )
        expect(navMocks.replace).toHaveBeenCalledWith(
            '/master-data/products/p9',
        )
    })

    it('protects unsaved changes when navigating away', async () => {
        mockedApi.fetchMasterDataCenter.mockResolvedValue(makeCenter())
        const { result } = renderEditor('p1')
        await waitFor(() => expect(result.current.data).toBeTruthy())

        act(() => {
            result.current.form.setFieldValue('changeReason', '更新名称')
        })
        act(() => {
            result.current.navigateAway('/master-data/products')
        })

        expect(navMocks.push).not.toHaveBeenCalled()
        expect(result.current.discardOpen).toBe(true)
        expect(result.current.pendingNav).toBe('/master-data/products')
    })

    it('navigates directly when the form is clean', () => {
        const { result } = renderEditor('new')

        act(() => {
            result.current.navigateAway('/master-data/products')
        })

        expect(navMocks.push).toHaveBeenCalledWith('/master-data/products')
        expect(result.current.discardOpen).toBe(false)
    })

    it('opens the inventory preview and refocuses the trigger on close', () => {
        const { result } = renderEditor('new')

        const trigger = document.createElement('button')
        act(() => {
            result.current.openInventoryPreview('sk1', trigger)
        })
        expect(result.current.inventoryOpen).toBe(true)
        expect(result.current.inventoryInitialSkuId).toBe('sk1')

        act(() => {
            result.current.handleInventoryOpenChange(false)
        })
        expect(result.current.inventoryOpen).toBe(false)
    })
})
