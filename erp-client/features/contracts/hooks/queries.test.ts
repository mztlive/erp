import { describe, it, expect, vi, beforeEach } from 'vitest'
import { waitFor } from '@testing-library/react'

import * as contractsApi from '@/features/contracts/api/contracts'
import {
    useContractCenterQuery,
    useContractsQuery,
    useCreateContractExportJobMutation,
    useUploadContractPdfMutation,
} from '@/features/contracts/hooks/queries'
import type {
    ContractCenterView,
    ContractExportJob,
    ContractListRow,
    UploadContractPdfResult,
} from '@/features/contracts/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/contracts/api/contracts', () => ({
    fetchContracts: vi.fn(),
    fetchContractCenter: vi.fn(),
    uploadContractPdf: vi.fn(),
    createContractExportJob: vi.fn(),
}))

const mockedApi = vi.mocked(contractsApi)

const listRow = (contractId: string): ContractListRow => ({
    contractId,
    contractNo: `CT-${contractId}`,
    customer: { customerId: 'c1', customerNo: 'C-001', displayName: '客户甲' },
    settlementParty: { partyId: 'p1', displayName: '主体乙' },
    status: 'EFFECTIVE',
    statusLabel: '生效',
    statusTone: 'success',
    revisionNo: 1,
    validFrom: '2026-01-01',
    validTo: '9999-12-31',
    expiringWithin30Days: false,
    salesOrderCount: 0,
    activeSalesOrderCount: 0,
    ownerLabel: '张三',
    ownerKind: 'current_customer_owner',
    allowedActions: ['PRINT', 'EXPORT'],
    actionBlockers: [],
})

describe('useContractsQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the contract list and exposes pending -> data', async () => {
        const rows = [listRow('a'), listRow('b')]
        mockedApi.fetchContracts.mockResolvedValue(rows)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useContractsQuery(),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        expect(mockedApi.fetchContracts).toHaveBeenCalledTimes(1)

        await waitFor(() => expect(result.current.data).toEqual(rows))
        expect(result.current.isSuccess).toBe(true)
    })

    it('uses the stable list query key and propagates errors', async () => {
        mockedApi.fetchContracts.mockRejectedValue(new Error('boom'))

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useContractsQuery(),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)

        const queries = client.getQueryCache().getAll()
        expect(queries).toHaveLength(1)
        expect(queries[0].queryKey).toEqual(['contracts', 'list'])
    })
})

describe('useContractCenterQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the center for a non-empty id with the detail key', async () => {
        const center = { contractId: 'c1' } as ContractCenterView
        mockedApi.fetchContractCenter.mockResolvedValue(center)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useContractCenterQuery('c1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(center))
        expect(mockedApi.fetchContractCenter).toHaveBeenCalledWith('c1')

        const queries = client.getQueryCache().getAll()
        expect(queries[0].queryKey).toEqual(['contracts', 'detail', 'c1'])
    })

    it('stays disabled and never fetches for an empty id', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useContractCenterQuery(''),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchContractCenter).not.toHaveBeenCalled()
    })

    it('surfaces null data without failing when the API returns null', async () => {
        mockedApi.fetchContractCenter.mockResolvedValue(null)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useContractCenterQuery('missing'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })
})

describe('useUploadContractPdfMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to uploadContractPdf and invalidates list/detail/selectable on success', async () => {
        const uploaded: UploadContractPdfResult = {
            contractId: 'ct1',
            contractNo: 'CT-1',
            revisionId: 'r1',
            revisionNo: 1,
            uploadedAt: '2026-01-01T00:00:00.000Z',
            fileName: 'a.pdf',
            reference: 'CT-UP-CT-1',
        }
        mockedApi.uploadContractPdf.mockResolvedValue(uploaded)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useUploadContractPdfMutation(),
            { queryClient: client },
        )

        const input = {
            pdfFile: new File(['x'], 'a.pdf', { type: 'application/pdf' }),
            contractNo: 'CT-1',
            customerId: 'c1',
            customerName: '客户甲',
            settlementPartyName: '主体乙',
            signedAt: '2026-01-01',
            validFrom: '2026-01-01',
            validTo: '2026-12-31',
            paymentTerms: 'CONTRACT',
            idempotencyKey: 'upload-x',
        }

        let value: UploadContractPdfResult | undefined
        await waitFor(async () => {
            value = await result.current.mutateAsync(input)
        })

        expect(mockedApi.uploadContractPdf).toHaveBeenCalledWith(input)
        expect(value).toEqual(uploaded)

        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(3))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['contracts', 'list'],
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['contracts', 'detail', 'ct1'],
        })
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['contracts', 'selectable-for-so'],
        })
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.uploadContractPdf.mockRejectedValue(new Error('fail'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useUploadContractPdfMutation(),
            { queryClient: client },
        )

        const input = {
            pdfFile: new File(['x'], 'a.pdf', { type: 'application/pdf' }),
            contractNo: 'CT-1',
            customerId: 'c1',
            customerName: '客户甲',
            settlementPartyName: '主体乙',
            signedAt: '2026-01-01',
            validFrom: '2026-01-01',
            validTo: '2026-12-31',
            paymentTerms: 'CONTRACT',
            idempotencyKey: 'upload-x',
        }

        await expect(
            result.current.mutateAsync(input),
        ).rejects.toThrow('fail')
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useCreateContractExportJobMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('passes input to createContractExportJob and returns the job', async () => {
        const job: ContractExportJob = {
            jobId: 'export_ct_1',
            status: 'queued',
            rowCount: 5,
            permissionVersion: 'pv-w04-1',
            filterSnapshotLabel: '指标=全部 · 搜索=空',
            createdAt: '2026-01-01T00:00:00.000Z',
            downloadLabel: '合同导出（5 行）',
        }
        mockedApi.createContractExportJob.mockResolvedValue(job)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCreateContractExportJobMutation(),
            { queryClient: client },
        )

        const value = await result.current.mutateAsync({
            rowCount: 5,
            filterSnapshotLabel: '指标=全部 · 搜索=空',
        })

        expect(mockedApi.createContractExportJob).toHaveBeenCalledTimes(1)
        expect(mockedApi.createContractExportJob.mock.calls[0][0]).toEqual({
            rowCount: 5,
            filterSnapshotLabel: '指标=全部 · 搜索=空',
        })
        expect(value).toEqual(job)
    })
})
