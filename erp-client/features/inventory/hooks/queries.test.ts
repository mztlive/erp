import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import * as inventoryApi from '@/features/inventory/api/inventory'
import {
    useBalanceDetailQuery,
    useCreateAdjustmentDraftMutation,
    useInventoryListQuery,
    useResolveAdjustmentUnknownMutation,
    useStartInventoryExportMutation,
    useSubmitAdjustmentMutation,
} from '@/features/inventory/hooks/queries'
import type {
    AdjustmentDraftView,
    BalanceDetailView,
    InventoryListView,
    InventoryQuery,
    StockBalanceRow,
} from '@/features/inventory/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/inventory/api/inventory', () => ({
    createAdjustmentDraft: vi.fn(),
    fetchBalanceDetail: vi.fn(),
    fetchInventoryList: vi.fn(),
    resolveAdjustmentUnknown: vi.fn(),
    startInventoryExport: vi.fn(),
    submitAdjustment: vi.fn(),
}))

const mockedApi = vi.mocked(inventoryApi)

const baseQuery: InventoryQuery = {
    view: 'balance',
    pageSize: 50,
    sort: ['warehouseCode:asc', 'skuCode:asc'],
}

function makeBalance(): StockBalanceRow {
    return {
        balanceId: 'b1',
        warehouseId: 'wh1',
        warehouseCode: 'WH01',
        warehouseName: '主仓',
        skuId: 'sku1',
        skuCode: 'SKU-1',
        skuName: '示例商品',
        specSummary: '500ml',
        baseUnit: '件',
        onHandQuantity: '10',
        reservedQuantity: '2',
        availableQuantity: '8',
        lockVersion: 1,
        lastMovementId: '',
        lastMovementAt: '',
        lastMovementTypeLabel: '',
        availability: 'positive',
        statusLabel: '有可用',
        statusTone: 'success',
        hasActiveReservation: false,
        stockKind: 'OWN_PHYSICAL',
        allowedActions: ['CREATE_ADJUSTMENT', 'VIEW_SOURCE'],
        actionBlockers: [],
    }
}

function makeListView(overrides: Partial<InventoryListView> = {}): InventoryListView {
    return {
        view: 'balance',
        metrics: {
            balanceDimensionCount: 1,
            reservedDimensionCount: 0,
            zeroAvailableDimensionCount: 0,
            pendingAdjustmentCount: 0,
        },
        balances: [makeBalance()],
        movements: [],
        reservations: [],
        adjustments: [],
        total: 1,
        cursor: '',
        pageSize: 50,
        sort: baseQuery.sort,
        filterSummary: '余额 · 全部仓库 · 全部状态 · 1 条',
        permissionVersion: 'pv-real',
        dataWatermark: '',
        lastMovementWatermark: '',
        queriedAt: '2026-08-14T00:00:00.000Z',
        hasWarehouseScope: true,
        moduleAllowed: true,
        canCreateAdjustment: true,
        canExport: true,
        emptyReason: undefined,
        excludedKindsNote: '',
        openingStockNote: '',
        warehouses: [{ id: 'wh1', code: 'WH01', name: 'WH01' }],
        ...overrides,
    }
}

function makeDetailView(): BalanceDetailView {
    return {
        balance: makeBalance(),
        recentMovements: [],
        reservations: [],
        sourceDocuments: [],
        pendingAdjustments: [],
        queriedAt: '2026-08-14T00:00:00.000Z',
    }
}

function makeDraftView(overrides: Partial<AdjustmentDraftView> = {}): AdjustmentDraftView {
    return {
        stockAdjustmentId: 'adj-1',
        adjustmentNo: 'TZ1',
        balanceId: 'b1',
        warehouseId: 'wh1',
        warehouseName: '主仓',
        skuId: 'sku1',
        skuCode: 'SKU-1',
        skuName: '示例商品',
        baseUnit: '件',
        reasonType: 'COUNT_LOSS',
        reasonTypeLabel: '盘亏',
        direction: 'decrease',
        quantity: '',
        note: '',
        occurredAt: '2026-08-14T00:00',
        status: 'DRAFT',
        statusLabel: '草稿',
        balanceLockVersion: 1,
        editVersion: 1,
        operatorLabel: '张三',
        segregationNote: '经办提交后进入仓储复核与财务确认',
        ...overrides,
    }
}

const submitInput = {
    stockAdjustmentId: 'adj-1',
    expectedBalanceLockVersion: 1,
    seedBalanceLockVersion: 1,
    reasonType: 'COUNT_LOSS' as const,
    reasonTypeLabel: '盘亏',
    direction: 'decrease' as const,
    quantity: '2',
    note: '盘点差异',
    occurredAt: '2026-08-14T00:00',
    idempotencyKey: 'idem-1',
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe('useInventoryListQuery', () => {
    it('fetches the list with the given query under a stable key', async () => {
        mockedApi.fetchInventoryList.mockResolvedValue(makeListView())

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useInventoryListQuery(baseQuery),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.data).toEqual(makeListView()),
        )
        expect(mockedApi.fetchInventoryList).toHaveBeenCalledWith(baseQuery)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['inventory', 'list', baseQuery],
        ])
    })

    it('reuses the cached result when the same query object re-renders', async () => {
        mockedApi.fetchInventoryList.mockResolvedValue(makeListView())

        const client = createFreshQueryClient()
        const { rerender } = renderHookWithProviders(
            () => useInventoryListQuery(baseQuery),
            { queryClient: client },
        )
        await waitFor(() => expect(mockedApi.fetchInventoryList).toHaveBeenCalledTimes(1))

        rerender()
        await waitFor(() => expect(client.getQueryCache().getAll()).toHaveLength(1))
        expect(mockedApi.fetchInventoryList).toHaveBeenCalledTimes(1)
    })

    it('does not fetch while disabled', () => {
        const { result } = renderHookWithProviders(
            () => useInventoryListQuery(baseQuery, false),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(result.current.isPending).toBe(true)
        expect(mockedApi.fetchInventoryList).not.toHaveBeenCalled()
    })

    it('surfaces error responses', async () => {
        mockedApi.fetchInventoryList.mockRejectedValue(new Error('boom'))

        const { result } = renderHookWithProviders(() =>
            useInventoryListQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toEqual(new Error('boom'))
    })

    it('serves an empty list without error', async () => {
        mockedApi.fetchInventoryList.mockResolvedValue(
            makeListView({ balances: [], total: 0, emptyReason: 'NO_DATA' }),
        )

        const { result } = renderHookWithProviders(() =>
            useInventoryListQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.total).toBe(0)
        expect(result.current.data?.balances).toHaveLength(0)
        expect(result.current.data?.emptyReason).toBe('NO_DATA')
    })
})

describe('useBalanceDetailQuery', () => {
    it('stays idle for a null balance id', () => {
        const { result } = renderHookWithProviders(() =>
            useBalanceDetailQuery(null),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchBalanceDetail).not.toHaveBeenCalled()
    })

    it('fetches the detail for the given balance id', async () => {
        mockedApi.fetchBalanceDetail.mockResolvedValue(makeDetailView())

        const { result } = renderHookWithProviders(() =>
            useBalanceDetailQuery('b1'),
        )

        await waitFor(() =>
            expect(result.current.data).toEqual(makeDetailView()),
        )
        expect(mockedApi.fetchBalanceDetail).toHaveBeenCalledWith('b1')
    })
})

describe('useCreateAdjustmentDraftMutation', () => {
    it('wires mutationFn to createAdjustmentDraft and invalidates inventory on success', async () => {
        const draft = makeDraftView()
        mockedApi.createAdjustmentDraft.mockResolvedValue(draft)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateAdjustmentDraftMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({ balanceId: 'b1' })
        })

        expect(mockedApi.createAdjustmentDraft).toHaveBeenCalledWith(
            { balanceId: 'b1' },
            expect.anything(),
        )
        await waitFor(() => expect(result.current.data).toEqual(draft))
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['inventory'],
            }),
        )
    })
})

describe('useSubmitAdjustmentMutation', () => {
    it('invalidates inventory queries when the submit succeeded', async () => {
        mockedApi.submitAdjustment.mockResolvedValue({
            status: 'succeeded',
            outcome: {
                kind: 'SUBMITTED_FOR_WAREHOUSE_REVIEW',
                stockAdjustmentId: 'adj-1',
                adjustmentNo: 'TZ1',
                nextResponsible: '仓储复核',
                reference: 'TZ1',
                submittedAt: '2026-08-14T00:00:00.000Z',
                balanceLockVersion: 1,
            },
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitAdjustmentMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(submitInput)
        })

        expect(mockedApi.submitAdjustment).toHaveBeenCalledWith(
            submitInput,
            expect.anything(),
        )
        await waitFor(() => expect(result.current.data?.status).toBe('succeeded'))
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['inventory'],
            }),
        )
    })

    it('does not invalidate when the submit failed', async () => {
        mockedApi.submitAdjustment.mockResolvedValue({
            status: 'failed',
            code: 'BALANCE_LOCK_CONFLICT',
            message: '数据已变更，请刷新后重试',
            latestLockVersion: 1,
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSubmitAdjustmentMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(submitInput)
        })

        await waitFor(() => expect(result.current.data?.status).toBe('failed'))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useResolveAdjustmentUnknownMutation', () => {
    it('wires mutationFn and invalidates when the outcome is succeeded', async () => {
        mockedApi.resolveAdjustmentUnknown.mockResolvedValue({
            status: 'succeeded',
            outcome: {
                kind: 'SUBMITTED_FOR_WAREHOUSE_REVIEW',
                stockAdjustmentId: 'adj-1',
                adjustmentNo: 'TZ1',
                nextResponsible: '仓储复核',
                reference: 'TZ1',
                submittedAt: '2026-08-14T00:00:00.000Z',
                balanceLockVersion: 1,
            },
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolveAdjustmentUnknownMutation(),
            { queryClient: client },
        )

        const input = {
            idempotencyKey: 'idem-1',
            stockAdjustmentId: 'adj-1',
            expectedBalanceLockVersion: 1,
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.resolveAdjustmentUnknown).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() => expect(result.current.data?.status).toBe('succeeded'))
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['inventory'],
            }),
        )
    })

    it('does not invalidate when the outcome is not succeeded', async () => {
        mockedApi.resolveAdjustmentUnknown.mockResolvedValue({
            status: 'failed',
            code: 'NO_PENDING',
            message: '未找到该任务号对应的处理中请求',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useResolveAdjustmentUnknownMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({ idempotencyKey: 'idem-1' })
        })

        await waitFor(() => expect(result.current.data?.status).toBe('failed'))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useStartInventoryExportMutation', () => {
    it('wires mutationFn to startInventoryExport and returns the job', async () => {
        const job = {
            jobId: 'INV-EXP-1',
            status: 'queued' as const,
            total: 42,
            completed: 0,
            filterSummary: '余额 · 全部仓库 · 42 条',
            createdAt: '2026-08-14T00:00:00.000Z',
            downloadLabel: undefined,
        }
        mockedApi.startInventoryExport.mockResolvedValue(job)

        const { result } = renderHookWithProviders(() =>
            useStartInventoryExportMutation(),
        )

        await act(async () => {
            await result.current.mutateAsync({
                total: 42,
                filterSummary: '余额 · 全部仓库 · 42 条',
            })
        })

        expect(mockedApi.startInventoryExport).toHaveBeenCalledWith(
            {
                total: 42,
                filterSummary: '余额 · 全部仓库 · 42 条',
            },
            expect.anything(),
        )
        await waitFor(() => expect(result.current.data).toEqual(job))
    })
})
