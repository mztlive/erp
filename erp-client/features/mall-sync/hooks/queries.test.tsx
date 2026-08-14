import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook, waitFor } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'

import * as mallSyncApi from '@/features/mall-sync/api/index'
import {
    useConfirmMappingMutation,
    useMallSyncPageQuery,
    useReapplyMutation,
    useRequestSourceFixMutation,
    useResolveUnknownReapplyMutation,
    useRetryJobMutation,
    useSourceSystemsQuery,
    useTriggerIncrementalMutation,
    useTriggerSingleOrderMutation,
} from '@/features/mall-sync/hooks/queries'
import type { MallSyncQueryInput } from '@/features/mall-sync/api/index'
import type { MallSyncPageView } from '@/features/mall-sync/types'
import { createFreshQueryClient } from '@/features/test-utils'

vi.mock('@/features/mall-sync/api/index', () => ({
    confirmMapping: vi.fn(),
    fetchMallSyncPage: vi.fn(),
    fetchSourceSystems: vi.fn(),
    reapplyMallSnapshot: vi.fn(),
    requestSourceFix: vi.fn(),
    resolveUnknownReapply: vi.fn(),
    retryFailedJob: vi.fn(),
    triggerManualIncremental: vi.fn(),
    triggerSingleOrderPull: vi.fn(),
}))

vi.mock('@/lib/api', () => ({
    isAuthenticated: vi.fn(),
}))

import { isAuthenticated } from '@/lib/api'

const mockedApi = vi.mocked(mallSyncApi)
const mockedIsAuthenticated = vi.mocked(isAuthenticated)

const baseInput: MallSyncQueryInput = {
    view: 'overview',
    q: undefined,
    owner: 'all',
}

function archivedPageView(): MallSyncPageView {
    return {
        context: {
            sourceSystem: {
                id: '',
                code: '',
                name: '未配置商城来源',
                environmentLabel: '—',
            },
            ownership: {
                businessType: 'VOUCHER',
                stage: 'ARCHIVED',
                originSystemSummary: 'MALL',
                syncDirection: 'SEALED_HISTORY',
                firstPhasePollingEnabled: false,
                mallWriteBoundary: '',
                erpWriteBoundary: '',
            },
            freshness: { viewProjectedAt: '2026-01-01T00:00:00.000Z' },
            metrics: [],
            hasSourceScope: false,
            scheduledIncrementalNote: '',
        },
        jobs: [],
        snapshots: [],
        mappingTasks: [],
        reconciliation: null,
        history: [],
    }
}

const succeededResult = {
    status: 'succeeded' as const,
    jobId: 'job-1',
    jobNo: 'JOB-1',
    message: 'ok',
}

describe('useMallSyncPageQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the page with the given input under a stable key', async () => {
        const client = createFreshQueryClient()
        mockedApi.fetchMallSyncPage.mockResolvedValue(archivedPageView())

        const { result } = renderHook(() => useMallSyncPageQuery(baseInput), {
            wrapper: makeWrapper(client),
        })

        expect(result.current.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.data?.context.hasSourceScope).toBe(false),
        )
        expect(mockedApi.fetchMallSyncPage).toHaveBeenCalledWith(baseInput)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['mall-sync', 'page', baseInput],
        ])
    })

    it('re-fetches the same key when the input changes', async () => {
        const client = createFreshQueryClient()
        mockedApi.fetchMallSyncPage.mockResolvedValue(archivedPageView())

        const first: MallSyncQueryInput = { ...baseInput, view: 'jobs' }
        const second: MallSyncQueryInput = { ...baseInput, view: 'snapshots' }
        const { result, rerender } = renderHook(
            (input: MallSyncQueryInput) => useMallSyncPageQuery(input),
            { wrapper: makeWrapper(client), initialProps: first },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchMallSyncPage).toHaveBeenCalledWith(first)

        rerender(second)
        await waitFor(() =>
            expect(mockedApi.fetchMallSyncPage).toHaveBeenCalledWith(second),
        )
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['mall-sync', 'page', first],
            ['mall-sync', 'page', second],
        ])
    })

    it('surfaces error responses', async () => {
        mockedApi.fetchMallSyncPage.mockRejectedValue(
            new Error('查询失败，请重试'),
        )

        const { result } = renderHook(() => useMallSyncPageQuery(baseInput), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
        expect(result.current.data).toBeUndefined()
    })
})

describe('useSourceSystemsQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches when authenticated with the given params', async () => {
        mockedIsAuthenticated.mockReturnValue(true)
        mockedApi.fetchSourceSystems.mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 20,
        })
        const params = { page: 2, page_size: 10 }
        const client = createFreshQueryClient()

        const { result } = renderHook(() => useSourceSystemsQuery(params), {
            wrapper: makeWrapper(client),
        })

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(mockedApi.fetchSourceSystems).toHaveBeenCalledWith(params)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['mall-sync', 'source-systems', params],
        ])
    })

    it('stays disabled (no request) when not authenticated', async () => {
        mockedIsAuthenticated.mockReturnValue(false)

        const { result } = renderHook(() => useSourceSystemsQuery(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchSourceSystems).not.toHaveBeenCalled()
    })

    it('uses first-page defaults when params are omitted', async () => {
        mockedIsAuthenticated.mockReturnValue(true)
        mockedApi.fetchSourceSystems.mockResolvedValue({
            items: [],
            total: 0,
            page: 1,
            page_size: 20,
        })

        renderHook(() => useSourceSystemsQuery(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })

        await waitFor(() =>
            expect(mockedApi.fetchSourceSystems).toHaveBeenCalledWith({
                page: 1,
                page_size: 20,
            }),
        )
    })
})

describe('sync mutations', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires triggerManualIncremental and returns its result', async () => {
        mockedApi.triggerManualIncremental.mockResolvedValue(succeededResult)

        const { result } = renderHook(() => useTriggerIncrementalMutation(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })

        const outcome = await result.current.mutateAsync({
            reason: '补全缺失数据',
        })
        expect(outcome.status).toBe('succeeded')
        expect(mockedApi.triggerManualIncremental.mock.calls[0][0]).toEqual({
            reason: '补全缺失数据',
        })
    })

    it('passes through a failed incremental trigger without calling the api again', async () => {
        mockedApi.triggerManualIncremental.mockResolvedValue({
            status: 'failed' as const,
            code: 'REASON_REQUIRED',
            message: 'reason too short',
        })

        const { result } = renderHook(() => useTriggerIncrementalMutation(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })

        const outcome = await result.current.mutateAsync({
            reason: '补全缺失数据',
        })
        expect(outcome.status).toBe('failed')
        expect(mockedApi.triggerManualIncremental).toHaveBeenCalledTimes(1)
    })

    it('invalidates the mall-sync cache when a trigger succeeds', async () => {
        mockedApi.triggerSingleOrderPull.mockResolvedValue(succeededResult)
        mockedApi.fetchMallSyncPage.mockResolvedValue(archivedPageView())
        const client = createFreshQueryClient()

        const page = renderHook(() => useMallSyncPageQuery(baseInput), {
            wrapper: makeWrapper(client),
        })
        await waitFor(() => expect(page.result.current.isSuccess).toBe(true))
        const callsBefore = mockedApi.fetchMallSyncPage.mock.calls.length

        const { result } = renderHook(() => useTriggerSingleOrderMutation(), {
            wrapper: makeWrapper(client),
        })
        await result.current.mutateAsync({
            externalOrderNo: 'SO-1',
            reason: '缺单补拉',
        })

        await waitFor(() =>
            expect(
                mockedApi.fetchMallSyncPage.mock.calls.length,
            ).toBeGreaterThan(callsBefore),
        )
    })
})

describe('mapping mutations', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires confirmMapping', async () => {
        mockedApi.confirmMapping.mockResolvedValue({
            status: 'succeeded' as const,
            mappingTaskId: 'mt-1',
            mappingTaskStatus: 'RESOLVED',
            externalIdentityMapId: 'eim-1',
            mappingTargetId: 'tgt-1',
            recordedAt: '2026-01-01T00:00:00.000Z',
            message: 'ok',
        })

        const { result } = renderHook(() => useConfirmMappingMutation(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })
        await result.current.mutateAsync({
            mappingTaskId: 'mt-1',
            sourceSnapshotId: 'snap-1',
            workItemId: 'wi-1',
            expectedTaskVersion: 'tv-1',
            expectedSubjectVersion: 'sv-1',
            expectedMappingTaskVersion: 2,
            mappingOperationId: 'op-1',
            targetObjectType: 'CUSTOMER',
            targetObjectId: 'c-1',
            relationRole: 'CUSTOMER',
            evidenceNote: '来源单客户与 ERP 客户一致',
            executionStage: 'FIRST_PHASE_MALL_OWNED',
            idempotencyKey: 'idem-1',
        })

        expect(mockedApi.confirmMapping).toHaveBeenCalledTimes(1)
    })

    it('wires requestSourceFix', async () => {
        mockedApi.requestSourceFix.mockResolvedValue({
            status: 'succeeded' as const,
            mappingTaskId: 'mt-1',
            mappingTaskStatus: 'PENDING',
            workItemStatus: 'OPEN',
            taskVersion: 'tv-2',
            mappingEvidenceEntryId: 'ev-1',
            recordedAt: '2026-01-01T00:00:00.000Z',
            message: 'ok',
        })

        const { result } = renderHook(() => useRequestSourceFixMutation(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })
        await result.current.mutateAsync({
            mappingTaskId: 'mt-1',
            sourceSnapshotId: 'snap-1',
            workItemId: 'wi-1',
            expectedTaskVersion: 'tv-1',
            expectedSubjectVersion: 'sv-1',
            expectedMappingTaskVersion: 2,
            requestOperationId: 'op-2',
            reasonCode: 'SOURCE_FIELD_MISSING',
            reasonText: '缺少来源字段',
            requestedEvidence: ['补充字段'],
            idempotencyKey: 'idem-2',
        })

        expect(mockedApi.requestSourceFix).toHaveBeenCalledTimes(1)
    })

    it('wires reapplyMallSnapshot', async () => {
        mockedApi.reapplyMallSnapshot.mockResolvedValue({
            status: 'succeeded' as const,
            operationId: 'op-3',
            reapplyOperationStatus: 'SUCCEEDED',
            salesOrderId: 'so-1',
            salesOrderRevisionId: 'rev-1',
            message: 'ok',
        })

        const { result } = renderHook(() => useReapplyMutation(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })
        await result.current.mutateAsync({
            mappingTaskId: 'mt-1',
            sourceSnapshotId: 'snap-1',
            expectedMappingVersion: 3,
            operationId: 'op-3',
            executionStage: 'FIRST_PHASE_MALL_OWNED',
            idempotencyKey: 'idem-3',
        })

        expect(mockedApi.reapplyMallSnapshot).toHaveBeenCalledTimes(1)
    })

    it('wires resolveUnknownReapply and keeps unknown results in cache scope', async () => {
        mockedApi.resolveUnknownReapply.mockResolvedValue({
            status: 'unknown' as const,
            reapplyOperationStatus: 'UNKNOWN',
            operationId: 'op-4',
            message: '结果仍未知',
            idempotencyKey: 'op-4',
        })

        const { result } = renderHook(
            () => useResolveUnknownReapplyMutation(),
            { wrapper: makeWrapper(createFreshQueryClient()) },
        )
        const outcome = await result.current.mutateAsync({
            mappingTaskId: 'mt-1',
            operationId: 'op-4',
            settle: true,
        })

        expect(outcome.status).toBe('unknown')
        expect(mockedApi.resolveUnknownReapply.mock.calls[0][0]).toEqual({
            mappingTaskId: 'mt-1',
            operationId: 'op-4',
            settle: true,
        })
    })

    it('wires retryFailedJob', async () => {
        mockedApi.retryFailedJob.mockResolvedValue(succeededResult)

        const { result } = renderHook(() => useRetryJobMutation(), {
            wrapper: makeWrapper(createFreshQueryClient()),
        })
        await result.current.mutateAsync({
            jobId: 'job-1',
            reason: '重试未成功部分的分页',
        })

        expect(mockedApi.retryFailedJob.mock.calls[0][0]).toEqual({
            jobId: 'job-1',
            reason: '重试未成功部分的分页',
        })
    })
})

function makeWrapper(client: ReturnType<typeof createFreshQueryClient>) {
    return ({ children }: { children: ReactNode }) => (
        <QueryClientProvider client={client}>{children}</QueryClientProvider>
    )
}
