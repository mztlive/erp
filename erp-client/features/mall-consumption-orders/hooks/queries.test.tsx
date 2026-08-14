import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import * as consumptionOrdersApi from '@/features/mall-consumption-orders/api/consumption-orders'
import {
    useConsumptionOrderDetailQuery,
    useConsumptionOrderExportMutation,
    useConsumptionOrderListQuery,
    useSalesOrderConsumptionSummaryQuery,
} from '@/features/mall-consumption-orders/hooks/queries'
import type {
    ExportCommand,
    ExportJobResult,
    MallConsumptionOrderListQuery,
    MallConsumptionOrderListResult,
    MallConsumptionOrderView,
    SalesOrderConsumptionSummary,
} from '@/features/mall-consumption-orders/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/mall-consumption-orders/api/consumption-orders', () => ({
    createConsumptionOrderExportJob: vi.fn(),
    fetchConsumptionOrderDetail: vi.fn(),
    fetchConsumptionOrderList: vi.fn(),
    fetchSalesOrderConsumptionSummary: vi.fn(),
}))

const mockedApi = vi.mocked(consumptionOrdersApi)

const baseQuery: MallConsumptionOrderListQuery = {
    occurredFrom: '2026-08-01',
    occurredTo: '2026-08-07',
    page: 1,
    pageSize: 8,
    sort: 'occurredAt.desc',
}

const listResult = (): MallConsumptionOrderListResult => ({
    rows: [],
    pageInfo: { page: 1, pageSize: 8, total: 0 },
    metrics: [],
    malls: [],
    filterSummary: '0 条',
    emptyReason: 'NO_DATA',
    hasModulePermission: true,
    hasDataScope: true,
    permissionVersion: 'server',
    dataScopeVersion: 'server',
    factWatermark: '2026-08-07T00:00:00.000Z',
    queriedAt: '2026-08-07T00:00:00.000Z',
    boundaryNotice: '只读',
})

describe('useSalesOrderConsumptionSummaryQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the summary for the given sales order id', async () => {
        const summary: SalesOrderConsumptionSummary = {
            salesOrderId: 'so-1',
            orderCount: 3,
            paidAmount: '12.00',
            refundedAmount: '1.00',
            restoredBalanceAmount: '0.00',
        }
        mockedApi.fetchSalesOrderConsumptionSummary.mockResolvedValue(summary)

        const { result } = renderHookWithProviders(() =>
            useSalesOrderConsumptionSummaryQuery('so-1'),
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchSalesOrderConsumptionSummary).toHaveBeenCalledWith(
            'so-1',
        )
        expect(result.current.data).toEqual(summary)
    })

    it('surfaces api failures as errors', async () => {
        mockedApi.fetchSalesOrderConsumptionSummary.mockRejectedValue(
            new Error('boom'),
        )

        const { result } = renderHookWithProviders(() =>
            useSalesOrderConsumptionSummaryQuery('so-1'),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useConsumptionOrderListQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('does not fetch while disabled', () => {
        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderListQuery(baseQuery, { enabled: false }),
        )

        expect(result.current.isPending).toBe(true)
        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchConsumptionOrderList).not.toHaveBeenCalled()
    })

    it('fetches with the given query and returns the result', async () => {
        mockedApi.fetchConsumptionOrderList.mockResolvedValue(listResult())

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderListQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchConsumptionOrderList).toHaveBeenCalledWith(
            baseQuery,
        )
        expect(result.current.data?.pageInfo.total).toBe(0)
        expect(result.current.data?.emptyReason).toBe('NO_DATA')
    })

    it('enables by default when no options are given', async () => {
        mockedApi.fetchConsumptionOrderList.mockResolvedValue(listResult())

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderListQuery(baseQuery),
        )

        await waitFor(() =>
            expect(mockedApi.fetchConsumptionOrderList).toHaveBeenCalledTimes(
                1,
            ),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
    })

    it('shares one cache entry for equal query objects', async () => {
        const client = createFreshQueryClient()
        const first = renderHookWithProviders(
            () => useConsumptionOrderListQuery(baseQuery),
            { queryClient: client },
        )
        await waitFor(() => expect(first.result.current.isSuccess).toBe(true))

        const second = renderHookWithProviders(
            () => useConsumptionOrderListQuery({ ...baseQuery }),
            { queryClient: client },
        )
        await waitFor(() => expect(second.result.current.isSuccess).toBe(true))

        // 结构相等的键共享同一条缓存记录。
        const queries = client.getQueryCache().getAll()
        expect(queries).toHaveLength(1)
        expect(queries[0].queryKey).toEqual([
            'mall-consumption-orders',
            'list',
            baseQuery,
        ])
    })
})

describe('useConsumptionOrderDetailQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('does not fetch for a null id', () => {
        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderDetailQuery(null),
        )

        expect(result.current.isPending).toBe(true)
        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchConsumptionOrderDetail).not.toHaveBeenCalled()
    })

    it('fetches the detail for the given id', async () => {
        const view = { identity: { mallOrderId: 'mo-1' } } as MallConsumptionOrderView
        mockedApi.fetchConsumptionOrderDetail.mockResolvedValue(view)

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderDetailQuery('mo-1'),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchConsumptionOrderDetail).toHaveBeenCalledWith(
            'mo-1',
        )
        expect(result.current.data).toEqual(view)
    })

    it('passes through a 404-mapped null result as data', async () => {
        mockedApi.fetchConsumptionOrderDetail.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderDetailQuery('missing'),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })
})

describe('useConsumptionOrderExportMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to createConsumptionOrderExportJob', async () => {
        const command: ExportCommand = {
            selectionSnapshotId: 'snap-1',
            fieldSetId: 'w25-list-default-masked',
            requestId: 'req-1',
            rowCount: 3,
            filterSummary: '3 条',
        }
        const job = {
            jobId: 'job-1',
            requestId: 'req-1',
            rowCount: 3,
            permissionVersion: 'server',
            fieldSetId: 'w25-list-default-masked',
            maskDisclaimer: '已打码',
            expiresAt: '2026-08-08T00:00:00.000Z',
            downloadLabel: '商城消费订单_job-1.csv',
            status: 'queued' as const,
        }
        mockedApi.createConsumptionOrderExportJob.mockResolvedValue(job)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useConsumptionOrderExportMutation(),
            { queryClient: client },
        )

        let outcome: ExportJobResult | undefined
        await act(async () => {
            outcome = await result.current.mutateAsync(command)
        })

        expect(outcome).toEqual(job)
        expect(mockedApi.createConsumptionOrderExportJob).toHaveBeenCalledWith(
            command,
        )
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith(
                expect.objectContaining({
                    queryKey: ['mall-consumption-orders'],
                }),
            ),
        )
    })

    it('propagates api failures through the mutation', async () => {
        mockedApi.createConsumptionOrderExportJob.mockRejectedValue(
            new Error('export failed'),
        )

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderExportMutation(),
        )

        await expect(
            act(() =>
                result.current.mutateAsync({
                    selectionSnapshotId: 'snap-1',
                    fieldSetId: 'w25-list-default-masked',
                    requestId: 'req-1',
                    rowCount: 1,
                    filterSummary: '1 条',
                }),
            ),
        ).rejects.toThrow('export failed')
        await waitFor(() => expect(result.current.isError).toBe(true))
    })
})
