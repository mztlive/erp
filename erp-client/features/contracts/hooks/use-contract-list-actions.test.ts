import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from '@testing-library/react'

import { useContractListActions } from '@/features/contracts/hooks/use-contract-list-actions'
import { useCreateContractExportJobMutation } from '@/features/contracts/hooks/queries'
import type {
    ContractExportJob,
    UploadContractPdfResult,
} from '@/features/contracts/types'
import { renderHookWithProviders } from '@/features/test-utils'

vi.mock('@/features/contracts/hooks/queries', () => ({
    useCreateContractExportJobMutation: vi.fn(),
}))

const mockedMutationHook = vi.mocked(useCreateContractExportJobMutation)

const uploadResult: UploadContractPdfResult = {
    contractId: 'ct-9',
    contractNo: 'CT-9',
    revisionId: 'r9',
    revisionNo: 2,
    uploadedAt: '2026-06-01T08:30:00.000Z',
    fileName: 'signed.pdf',
    reference: 'CT-UP-CT-9',
}

const exportJob: ContractExportJob = {
    jobId: 'export_ct_1',
    status: 'queued',
    rowCount: 12,
    permissionVersion: 'pv-w04-1',
    filterSnapshotLabel: '指标=全部 · 搜索=空',
    createdAt: '2026-06-01T09:00:00.000Z',
    downloadLabel: '合同导出（12 行）',
}

describe('useContractListActions', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        mockedMutationHook.mockReturnValue({
            mutateAsync: vi.fn().mockResolvedValue(exportJob),
            isPending: false,
        } as unknown as ReturnType<typeof useCreateContractExportJobMutation>)
    })

    it('records the upload result with facts and a detail link', () => {
        const { result } = renderHookWithProviders(
            () =>
                useContractListActions({
                    filteredCount: 0,
                    filterSnapshotLabel: '指标=全部 · 搜索=空',
                }),
            {},
        )

        act(() => {
            result.current.handleUploadSuccess(uploadResult)
        })

        expect(result.current.actionResult).toEqual({
            status: 'succeeded',
            title: '合同 PDF 已归档',
            description:
                '已形成可追溯的合同版本，可直接选择用于新建销售单。',
            facts: [
                { label: '合同号', value: 'CT-9' },
                { label: '修订', value: 'v2' },
                { label: '文件', value: 'signed.pdf' },
                { label: '上传时间', value: '2026-06-01 08:30:00' },
                { label: '下一步', value: '查看详情核对或新建销售单' },
            ],
            nextHref: '/sales/contracts/ct-9',
        })
        expect(result.current.exportJob).toBeNull()
    })

    it('skips export when the filtered set is empty', async () => {
        const mutateAsync = vi.fn()
        mockedMutationHook.mockReturnValue({
            mutateAsync,
            isPending: false,
        } as unknown as ReturnType<typeof useCreateContractExportJobMutation>)

        const { result } = renderHookWithProviders(
            () =>
                useContractListActions({
                    filteredCount: 0,
                    filterSnapshotLabel: '指标=全部 · 搜索=空',
                }),
            {},
        )

        await act(async () => {
            await result.current.handleExport()
        })

        expect(mutateAsync).not.toHaveBeenCalled()
        expect(result.current.exportJob).toBeNull()
        expect(result.current.actionResult).toBeNull()
    })

    it('runs the export mutation and records job + result on success', async () => {
        const mutateAsync = vi.fn().mockResolvedValue(exportJob)
        mockedMutationHook.mockReturnValue({
            mutateAsync,
            isPending: false,
        } as unknown as ReturnType<typeof useCreateContractExportJobMutation>)

        const { result } = renderHookWithProviders(
            () =>
                useContractListActions({
                    filteredCount: 12,
                    filterSnapshotLabel: '指标=有效 · 搜索=客户甲',
                }),
            {},
        )

        await act(async () => {
            await result.current.handleExport()
        })

        expect(mutateAsync).toHaveBeenCalledWith({
            rowCount: 12,
            filterSnapshotLabel: '指标=有效 · 搜索=客户甲',
        })
        expect(result.current.exportJob).toEqual(exportJob)
        expect(result.current.actionResult).toEqual({
            status: 'succeeded',
            title: '导出完成',
            description:
                '已生成 CSV 文件，内容按当前筛选生成；下载时将重新校验权限。',
            facts: [
                { label: '筛选结果', value: '指标=全部 · 搜索=空' },
                { label: '行数', value: '12' },
                { label: '文件', value: '合同导出（12 行）' },
            ],
        })
    })

    it('propagates export errors without recording results', async () => {
        mockedMutationHook.mockReturnValue({
            mutateAsync: vi.fn().mockRejectedValue(new Error('boom')),
            isPending: false,
        } as unknown as ReturnType<typeof useCreateContractExportJobMutation>)

        const { result } = renderHookWithProviders(
            () =>
                useContractListActions({
                    filteredCount: 3,
                    filterSnapshotLabel: '指标=全部 · 搜索=空',
                }),
            {},
        )

        await expect(
            act(async () => {
                await result.current.handleExport()
            }),
        ).rejects.toThrow('boom')
        expect(result.current.exportJob).toBeNull()
        expect(result.current.actionResult).toBeNull()
    })
})
