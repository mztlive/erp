import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import * as consumptionOrdersApi from '@/features/mall-consumption-orders/api/consumption-orders'
import { useConsumptionOrderExportFlow } from '@/features/mall-consumption-orders/hooks/use-consumption-order-export'
import type { ExportJobResult } from '@/features/mall-consumption-orders/types'
import { renderHookWithProviders } from '@/features/test-utils'

vi.mock('@/features/mall-consumption-orders/api/consumption-orders', () => ({
    createConsumptionOrderExportJob: vi.fn(),
    fetchConsumptionOrderDetail: vi.fn(),
    fetchConsumptionOrderList: vi.fn(),
    fetchSalesOrderConsumptionSummary: vi.fn(),
}))

const mockedApi = vi.mocked(consumptionOrdersApi)

const jobResult = (): ExportJobResult => ({
    jobId: 'job-1',
    requestId: 'req-x',
    rowCount: 5,
    permissionVersion: 'server',
    fieldSetId: 'w25-list-default-masked',
    maskDisclaimer: '已打码',
    expiresAt: '2026-08-08T00:00:00.000Z',
    downloadLabel: '商城消费订单_job-1.csv',
    status: 'queued',
})

describe('useConsumptionOrderExportFlow', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('starts closed with no result', () => {
        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderExportFlow(5, '5 条'),
        )

        expect(result.current.exportPreviewOpen).toBe(false)
        expect(result.current.exportResult).toBeNull()
    })

    it('confirmExport posts the command with the current totals and shows the result', async () => {
        mockedApi.createConsumptionOrderExportJob.mockResolvedValue(
            jobResult(),
        )

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderExportFlow(5, '支付成功 · 5 条'),
        )

        act(() => {
            result.current.openExportPreview()
        })
        expect(result.current.exportPreviewOpen).toBe(true)

        await act(async () => {
            await result.current.confirmExport()
        })

        expect(mockedApi.createConsumptionOrderExportJob).toHaveBeenCalledTimes(
            1,
        )
        const command = mockedApi.createConsumptionOrderExportJob.mock
            .calls[0][0]
        expect(command).toMatchObject({
            selectionSnapshotId: expect.stringMatching(/^snap-req-w25-export-/),
            fieldSetId: 'w25-list-default-masked',
            requestId: expect.stringMatching(/^req-w25-export-\d+$/),
            rowCount: 5,
            filterSummary: '支付成功 · 5 条',
        })

        expect(result.current.exportPreviewOpen).toBe(false)
        expect(result.current.exportResult).toEqual({
            jobId: 'job-1',
            rowCount: 5,
            permissionVersion: 'server',
            maskDisclaimer: '已打码',
            downloadLabel: '商城消费订单_job-1.csv',
            expiresAt: '2026-08-08T00:00:00.000Z',
        })
    })

    it('uses zero totals when the list has no data', async () => {
        mockedApi.createConsumptionOrderExportJob.mockResolvedValue(
            jobResult(),
        )

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderExportFlow(0, ''),
        )

        await act(async () => {
            await result.current.confirmExport()
        })

        expect(
            mockedApi.createConsumptionOrderExportJob.mock.calls[0][0],
        ).toMatchObject({ rowCount: 0, filterSummary: '' })
        expect(result.current.exportResult?.rowCount).toBe(5)
    })

    it('cancelExportPreview closes the preview without posting', () => {
        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderExportFlow(5, '5 条'),
        )

        act(() => {
            result.current.openExportPreview()
        })
        expect(result.current.exportPreviewOpen).toBe(true)

        act(() => {
            result.current.cancelExportPreview()
        })
        expect(result.current.exportPreviewOpen).toBe(false)
        expect(mockedApi.createConsumptionOrderExportJob).not.toHaveBeenCalled()
    })

    it('leaves the preview open and surfaces the error when the export fails', async () => {
        mockedApi.createConsumptionOrderExportJob.mockRejectedValue(
            new Error('export failed'),
        )

        const { result } = renderHookWithProviders(() =>
            useConsumptionOrderExportFlow(5, '5 条'),
        )
        act(() => {
            result.current.openExportPreview()
        })

        await expect(
            act(async () => {
                await result.current.confirmExport()
            }),
        ).rejects.toThrow('export failed')

        expect(result.current.exportPreviewOpen).toBe(true)
        expect(result.current.exportResult).toBeNull()
        await waitFor(() =>
            expect(result.current.exportMutation.isError).toBe(true),
        )
    })
})
