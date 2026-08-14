import { act, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { createFreshQueryClient, renderHookWithProviders } from '@/features/test-utils'
import type { CardBusinessAnalyticsQuery } from '../types'
import { makeStubView } from './test-data'
import { useCardBusinessExport } from './use-card-business-export'

vi.mock('@/features/card-business-analytics/api/card-business-analytics', () => ({
    fetchDateBasisConfig: vi.fn(),
    fetchCardBusinessAnalytics: vi.fn(),
    startCardBusinessExport: vi.fn(),
}))

import {
    startCardBusinessExport,
} from '@/features/card-business-analytics/api/card-business-analytics'

const mockedStartCardBusinessExport = vi.mocked(startCardBusinessExport)

const analysisQuery: CardBusinessAnalyticsQuery = {
    from: '2026-08-01',
    to: '2026-08-07',
    dateBasis: 'consumption',
    dimension: 'customer',
    sort: 'consumption:desc',
    page: 1,
    pageSize: 50,
}

const exportJobStub = {
    jobId: 'job-1',
    status: 'queued' as const,
    total: 1,
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
        rowCount: 1,
    },
}

beforeEach(() => {
    mockedStartCardBusinessExport.mockReset()
})

describe('useCardBusinessExport', () => {
    it('is a no-op when data or the analysis query are missing', async () => {
        const { result } = renderHookWithProviders(() =>
            useCardBusinessExport({ data: undefined, analysisQuery: null }),
        )
        await act(async () => {
            await result.current.handleExportConfirm()
        })
        expect(mockedStartCardBusinessExport).not.toHaveBeenCalled()
        expect(result.current.exportJob).toBeNull()
    })

    it('confirms the export via the mutation and stores the job', async () => {
        const data = makeStubView()
        mockedStartCardBusinessExport.mockResolvedValue(exportJobStub)
        const { result } = renderHookWithProviders(() =>
            useCardBusinessExport({ data, analysisQuery }),
        )
        act(() => {
            result.current.setExportPreviewOpen(true)
        })
        expect(result.current.exportPreviewOpen).toBe(true)
        await act(async () => {
            await result.current.handleExportConfirm()
        })
        expect(result.current.exportPreviewOpen).toBe(false)
        expect(mockedStartCardBusinessExport).toHaveBeenCalledWith(
            {
                query: analysisQuery,
                view: {
                    period: data.period,
                    scope: data.scope,
                    freshness: data.freshness,
                    coverage: data.coverage,
                    filterSummary: data.filterSummary,
                    wechatExcludedNote: data.wechatExcludedNote,
                    fieldPermissions: data.fieldPermissions,
                    rows: data.rows,
                },
            },
            expect.anything(),
        )
        expect(result.current.exportJob).toEqual(exportJobStub)
    })

    it('exposes isExporting while the mutation is in flight', async () => {
        const data = makeStubView()
        let resolveExport: (job: typeof exportJobStub) => void = () => {}
        mockedStartCardBusinessExport.mockImplementation(
            () =>
                new Promise((resolve) => {
                    resolveExport = resolve
                }),
        )
        const { result } = renderHookWithProviders(() =>
            useCardBusinessExport({ data, analysisQuery }),
        )
        act(() => {
            void result.current.handleExportConfirm()
        })
        await waitFor(() => expect(result.current.isExporting).toBe(true))
        expect(result.current.exportJob).toBeNull()
        act(() => {
            resolveExport(exportJobStub)
        })
        await waitFor(() => expect(result.current.exportJob).toEqual(exportJobStub))
        expect(result.current.isExporting).toBe(false)
    })

    it('leaves the job empty when the export fails', async () => {
        const data = makeStubView()
        mockedStartCardBusinessExport.mockRejectedValue(
            new Error('export denied'),
        )
        const { result } = renderHookWithProviders(() =>
            useCardBusinessExport({ data, analysisQuery }),
        )
        await act(async () => {
            await expect(result.current.handleExportConfirm()).rejects.toThrow(
                'export denied',
            )
        })
        expect(result.current.exportJob).toBeNull()
    })

    it('replaces the job and closes it on demand', () => {
        const client = createFreshQueryClient()
        const data = makeStubView()
        const { result } = renderHookWithProviders(
            () => useCardBusinessExport({ data, analysisQuery }),
            { queryClient: client },
        )
        act(() => {
            result.current.setExportJob(exportJobStub)
        })
        expect(result.current.exportJob?.jobId).toBe('job-1')
        act(() => {
            result.current.setExportJob(null)
        })
        expect(result.current.exportJob).toBeNull()
    })
})
