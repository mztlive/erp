import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act } from '@testing-library/react'

import { renderHookWithProviders } from '@/features/test-utils'
import { useInventoryExportJob } from './use-inventory-export-job'

const { exportMutateAsyncMock } = vi.hoisted(() => ({
    exportMutateAsyncMock: vi.fn(),
}))

vi.mock('@/features/inventory/hooks/queries', () => ({
    useStartInventoryExportMutation: () => ({
        mutateAsync: exportMutateAsyncMock,
        isPending: false,
    }),
}))

const jobFixture = {
    jobId: 'INV-EXP-1',
    status: 'queued' as const,
    total: 3,
    completed: 0,
    filterSummary: '余额 · 全部仓库',
    createdAt: '2026-08-14T00:00:00.000Z',
}

function setup() {
    const rendered = renderHookWithProviders(() => useInventoryExportJob())
    return rendered
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe('useInventoryExportJob', () => {
    it('starts with no export job', () => {
        const { result } = setup()
        expect(result.current.exportJob).toBeNull()
    })

    it('startExport calls the mutation with totals and filter summary, then stores the job', async () => {
        exportMutateAsyncMock.mockResolvedValue(jobFixture)
        const { result } = setup()

        await act(async () => {
            result.current.startExport({
                total: 12,
                filterSummary: '余额 · 全部仓库',
            })
        })
        expect(exportMutateAsyncMock).toHaveBeenCalledWith({
            total: 12,
            filterSummary: '余额 · 全部仓库',
        })
        expect(result.current.exportJob).toEqual(jobFixture)
    })

    it('closeExport clears the job', async () => {
        exportMutateAsyncMock.mockResolvedValue(jobFixture)
        const { result } = setup()
        await act(async () => {
            result.current.startExport({
                total: 12,
                filterSummary: '余额 · 全部仓库',
            })
        })
        expect(result.current.exportJob).not.toBeNull()

        act(() => {
            result.current.closeExport()
        })
        expect(result.current.exportJob).toBeNull()
    })
})
