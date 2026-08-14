import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { createFreshQueryClient, renderHookWithProviders } from '@/features/test-utils'
import type { CardBusinessAnalyticsQuery } from '../types'
import { makeStubView } from './test-data'
import {
    useCardBusinessAnalyticsQuery,
    useDateBasisConfigQuery,
    useStartCardBusinessExportMutation,
} from './queries'

vi.mock('@/features/card-business-analytics/api/card-business-analytics', () => ({
    fetchDateBasisConfig: vi.fn(),
    fetchCardBusinessAnalytics: vi.fn(),
    startCardBusinessExport: vi.fn(),
}))

import {
    fetchCardBusinessAnalytics,
    fetchDateBasisConfig,
    startCardBusinessExport,
} from '@/features/card-business-analytics/api/card-business-analytics'

const mockedFetchDateBasisConfig = vi.mocked(fetchDateBasisConfig)
const mockedFetchCardBusinessAnalytics = vi.mocked(fetchCardBusinessAnalytics)
const mockedStartCardBusinessExport = vi.mocked(startCardBusinessExport)

const basisConfigStub = {
    configuredDateBasis: 'consumption' as const,
    allowedDateBases: [
        { code: 'consumption' as const, label: '消费发生日', explanation: '' },
        { code: 'sales' as const, label: '销售发生日', explanation: '' },
        { code: 'expiry' as const, label: '履约到期日', explanation: '' },
    ],
    configurationVersion: 'v1',
}

const fullQuery: CardBusinessAnalyticsQuery = {
    from: '2026-08-01',
    to: '2026-08-07',
    dateBasis: 'consumption',
    dimension: 'customer',
    sort: 'consumption:desc',
    page: 1,
    pageSize: 50,
}

beforeEach(() => {
    mockedFetchDateBasisConfig.mockReset()
    mockedFetchCardBusinessAnalytics.mockReset()
    mockedStartCardBusinessExport.mockReset()
})

describe('useDateBasisConfigQuery', () => {
    it('fetches the config with the given query arg and exposes data', async () => {
        mockedFetchDateBasisConfig.mockResolvedValue(basisConfigStub)
        const { result } = renderHookWithProviders(() =>
            useDateBasisConfigQuery({ scenario: 'default' }),
        )
        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedFetchDateBasisConfig).toHaveBeenCalledWith({
            scenario: 'default',
        })
        expect(result.current.data?.configuredDateBasis).toBe('consumption')
        expect(result.current.data?.allowedDateBases).toHaveLength(3)
    })

    it('defaults the query arg to an empty object', async () => {
        mockedFetchDateBasisConfig.mockResolvedValue(basisConfigStub)
        const { result } = renderHookWithProviders(() =>
            useDateBasisConfigQuery(),
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedFetchDateBasisConfig).toHaveBeenCalledWith({})
    })

    it('does not refetch while the result is still within staleTime', async () => {
        const client = createFreshQueryClient()
        mockedFetchDateBasisConfig.mockResolvedValue(basisConfigStub)
        const first = renderHookWithProviders(() => useDateBasisConfigQuery(), {
            queryClient: client,
        })
        await waitFor(() => expect(first.result.current.isSuccess).toBe(true))
        const second = renderHookWithProviders(() => useDateBasisConfigQuery(), {
            queryClient: client,
        })
        await waitFor(() => expect(second.result.current.isSuccess).toBe(true))
        expect(mockedFetchDateBasisConfig).toHaveBeenCalledTimes(1)
    })

    it('propagates api errors to the error state', async () => {
        mockedFetchDateBasisConfig.mockRejectedValue(new Error('config down'))
        const { result } = renderHookWithProviders(() =>
            useDateBasisConfigQuery(),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error?.message).toBe('config down')
    })
})

describe('useCardBusinessAnalyticsQuery', () => {
    it('stays idle and never calls the api when the query is null', async () => {
        const { result } = renderHookWithProviders(() =>
            useCardBusinessAnalyticsQuery(null, false),
        )
        expect(result.current.isPending).toBe(true)
        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedFetchCardBusinessAnalytics).not.toHaveBeenCalled()
    })

    it('stays disabled when enabled but from/to/dateBasis are incomplete', async () => {
        const incomplete: CardBusinessAnalyticsQuery = { ...fullQuery, to: '' }
        const { result } = renderHookWithProviders(() =>
            useCardBusinessAnalyticsQuery(incomplete, true),
        )
        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedFetchCardBusinessAnalytics).not.toHaveBeenCalled()
    })

    it('passes the query object verbatim to the api', async () => {
        mockedFetchCardBusinessAnalytics.mockResolvedValue(makeStubView())
        const { result } = renderHookWithProviders(() =>
            useCardBusinessAnalyticsQuery(fullQuery, true),
        )
        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedFetchCardBusinessAnalytics).toHaveBeenCalledWith(fullQuery)
        expect(result.current.data?.rows.total).toBe(1)
    })

    it('shares cached results for the same query shape and separates different shapes', async () => {
        const client = createFreshQueryClient()
        client.setQueryDefaults(['card-business-analytics'], {
            staleTime: Number.POSITIVE_INFINITY,
        })
        mockedFetchCardBusinessAnalytics.mockResolvedValue(makeStubView())
        const first = renderHookWithProviders(
            () => useCardBusinessAnalyticsQuery(fullQuery, true),
            { queryClient: client },
        )
        await waitFor(() => expect(first.result.current.isSuccess).toBe(true))

        // 相同查询形状（值相等）→ 命中缓存，不再请求
        const sameShape = renderHookWithProviders(
            () => useCardBusinessAnalyticsQuery({ ...fullQuery }, true),
            { queryClient: client },
        )
        await waitFor(() => expect(sameShape.result.current.isSuccess).toBe(true))
        expect(mockedFetchCardBusinessAnalytics).toHaveBeenCalledTimes(1)

        // 不同查询形状 → 新请求
        const otherShape = renderHookWithProviders(
            () => useCardBusinessAnalyticsQuery({ ...fullQuery, page: 2 }, true),
            { queryClient: client },
        )
        await waitFor(() => expect(otherShape.result.current.isSuccess).toBe(true))
        expect(mockedFetchCardBusinessAnalytics).toHaveBeenCalledTimes(2)
    })

    it('propagates api errors to the error state', async () => {
        mockedFetchCardBusinessAnalytics.mockRejectedValue(
            new Error('view down'),
        )
        const { result } = renderHookWithProviders(() =>
            useCardBusinessAnalyticsQuery(fullQuery, true),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error?.message).toBe('view down')
    })
})

describe('useStartCardBusinessExportMutation', () => {
    it('wires mutationFn to startCardBusinessExport and resolves its result', async () => {
        const job = {
            jobId: 'job-1',
            status: 'queued' as const,
            total: 10,
            completed: 0,
            createdAt: '2026-08-07T10:00:00Z',
            watermark: {
                periodFrom: '2026-08-01',
                periodTo: '2026-08-07',
                dateBasis: 'consumption' as const,
                filterSummary: '期间 2026-08-01 ~ 2026-08-07',
                coverageRate: '80%',
                projectionUpdatedAt: '2026-08-07T10:00:00Z',
                consumedOutboxWatermark: '2026-08-07T09:59:00Z',
                lagSeconds: 30,
                permissionVersion: 'v1',
                taxDisclaimer: '免责声明',
                wechatExcludedNote: '',
                rowCount: 10,
            },
        }
        mockedStartCardBusinessExport.mockResolvedValue(job)
        const payload = { query: fullQuery, view: makeStubView() }
        const { result } = renderHookWithProviders(() =>
            useStartCardBusinessExportMutation(),
        )
        expect(result.current.isPending).toBe(false)
        act(() => {
            result.current.mutate(payload)
        })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedStartCardBusinessExport).toHaveBeenCalledWith(
            payload,
            expect.anything(),
        )
        expect(result.current.data?.jobId).toBe('job-1')
    })

    it('surfaces mutation errors', async () => {
        mockedStartCardBusinessExport.mockRejectedValue(
            new Error('export denied'),
        )
        const { result } = renderHookWithProviders(() =>
            useStartCardBusinessExportMutation(),
        )
        act(() => {
            result.current.mutate({
                query: fullQuery,
                view: makeStubView(),
            })
        })
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error?.message).toBe('export denied')
    })
})
