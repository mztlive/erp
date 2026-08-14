import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act, renderHook, waitFor } from '@testing-library/react'
import { QueryClientProvider } from '@tanstack/react-query'
import type { ReactNode } from 'react'

import * as customersApi from '@/features/customers/api/index'
import {
    useApplyCustomerAssignmentMutation,
    useCreateCustomerMutation,
    useCustomerCenterQuery,
    useCustomerDirectoryQuery,
    useQueryCustomerIdempotencyMutation,
    useSaveCustomerDetailsMutation,
} from '@/features/customers/hooks/queries'
import type {
    CustomerCenterView,
    CustomerDirectoryQuery,
    CustomerDirectoryResult,
    CustomerMutationResult,
} from '@/features/customers/types'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'

vi.mock('@/features/customers/api/index', () => ({
    applyCustomerAssignment: vi.fn(),
    createCustomer: vi.fn(),
    fetchCustomerCenter: vi.fn(),
    fetchCustomerDirectory: vi.fn(),
    queryCustomerMutationByIdempotency: vi.fn(),
    revealCustomerSensitiveField: vi.fn(),
    saveCustomerDetails: vi.fn(),
}))

const mockedApi = vi.mocked(customersApi)

const baseQuery: CustomerDirectoryQuery = {
    scope: 'mine',
    status: 'all',
    page: 1,
    pageSize: 20,
}

const directoryResult = (page: number): CustomerDirectoryResult => ({
    hasCustomerScope: true,
    items: [],
    totalInScope: 0,
    page,
    pageSize: 20,
    queriedAt: '2026-01-01T00:00:00.000Z',
})

const succeededResult = (customerNo: string): CustomerMutationResult => ({
    outcome: 'succeeded',
    customerId: 'c1',
    customerNo,
    revisionNo: 2,
    lockVersion: 3,
    occurredAt: '2026-01-01T00:00:00.000Z',
    reference: 'C-1-R2',
})

describe('useCustomerDirectoryQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the directory with the given query under a stable key', async () => {
        mockedApi.fetchCustomerDirectory.mockResolvedValue(
            directoryResult(1),
        )

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerDirectoryQuery(baseQuery),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)

        await waitFor(() =>
            expect(result.current.data).toEqual(directoryResult(1)),
        )
        expect(mockedApi.fetchCustomerDirectory).toHaveBeenCalledWith(
            baseQuery,
        )
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['customers', 'directory', baseQuery],
        ])
    })

    it('keeps the previous page rendered while a new query loads', async () => {
        mockedApi.fetchCustomerDirectory
            .mockResolvedValueOnce(directoryResult(1))
            .mockResolvedValueOnce(directoryResult(2))

        const client = createFreshQueryClient()
        const wrapper = ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        )
        const { result, rerender } = renderHook(
            ({ query }: { query: CustomerDirectoryQuery }) =>
                useCustomerDirectoryQuery(query),
            { wrapper, initialProps: { query: baseQuery } },
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))

        act(() => {
            rerender({ query: { ...baseQuery, page: 2 } })
        })

        expect(result.current.isPlaceholderData).toBe(true)
        expect(result.current.data).toEqual(directoryResult(1))
        await waitFor(() =>
            expect(result.current.data).toEqual(directoryResult(2)),
        )
        expect(result.current.isPlaceholderData).toBe(false)
    })

    it('stays disabled and never fetches when disabled is set', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerDirectoryQuery(baseQuery, { enabled: false }),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchCustomerDirectory).not.toHaveBeenCalled()
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchCustomerDirectory.mockRejectedValue(new Error('boom'))

        const { result } = renderHookWithProviders(() =>
            useCustomerDirectoryQuery(baseQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useCustomerCenterQuery', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('fetches the customer center under the detail key', async () => {
        const center = { customerId: 'c1' } as CustomerCenterView
        mockedApi.fetchCustomerCenter.mockResolvedValue(center)

        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerCenterQuery('c1'),
            { queryClient: client },
        )

        await waitFor(() => expect(result.current.data).toEqual(center))
        expect(mockedApi.fetchCustomerCenter).toHaveBeenCalledWith('c1')
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ['customers', 'detail', 'c1'],
        ])
    })

    it('stays disabled and never fetches for an empty id', () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useCustomerCenterQuery(''),
            { queryClient: client },
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedApi.fetchCustomerCenter).not.toHaveBeenCalled()
    })

    it('surfaces null data when the api returns null (missing or forbidden)', async () => {
        mockedApi.fetchCustomerCenter.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            useCustomerCenterQuery('missing'),
        )

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })

    it('propagates errors from the api', async () => {
        mockedApi.fetchCustomerCenter.mockRejectedValue(new Error('down'))

        const { result } = renderHookWithProviders(() =>
            useCustomerCenterQuery('c1'),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useCreateCustomerMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to createCustomer and invalidates all customer queries on success', async () => {
        mockedApi.createCustomer.mockResolvedValue(succeededResult('C-1'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateCustomerMutation(),
            { queryClient: client },
        )

        const input = {
            legalName: '示例有限公司',
            unifiedCreditCode: '91310000XXXXXXXXXX',
            idempotencyKey: 'create-abc',
        }
        let value: CustomerMutationResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(input)
        })

        expect(mockedApi.createCustomer).toHaveBeenCalledWith(input)
        expect(value).toEqual(succeededResult('C-1'))
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customers'],
        })
    })

    it('skips invalidation when the outcome is not succeeded', async () => {
        const conflict: CustomerMutationResult = {
            outcome: 'conflict',
            message: '名称相似',
            serverLockVersion: 0,
            serverRevisionNo: 0,
            serverLegalName: '示例有限公司',
            actor: '系统',
            changedAt: '2026-01-01T00:00:00.000Z',
        }
        mockedApi.createCustomer.mockResolvedValue(conflict)

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateCustomerMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync({
                legalName: '示例有限公司',
                unifiedCreditCode: '91310000XXXXXXXXXX',
                idempotencyKey: 'create-abc',
            })
        })

        expect(invalidateSpy).not.toHaveBeenCalled()
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.createCustomer.mockRejectedValue(new Error('fail'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateCustomerMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current
                .mutateAsync({
                    legalName: '示例有限公司',
                    unifiedCreditCode: '91310000XXXXXXXXXX',
                    idempotencyKey: 'create-abc',
                })
                .catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })

    it('keeps unknown outcomes visible without invalidating', async () => {
        mockedApi.createCustomer.mockResolvedValue({
            outcome: 'unknown',
            message: '处理结果待确认，请勿重复提交',
            idempotencyKey: 'create-z',
        })

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useCreateCustomerMutation(),
            { queryClient: client },
        )

        let value: CustomerMutationResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync({
                legalName: '示例有限公司',
                unifiedCreditCode: '91310000XXXXXXXXXX',
                idempotencyKey: 'create-z',
            })
        })

        expect(value?.outcome).toBe('unknown')
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useSaveCustomerDetailsMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to saveCustomerDetails and invalidates on success', async () => {
        mockedApi.saveCustomerDetails.mockResolvedValue(
            succeededResult('C-1'),
        )

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSaveCustomerDetailsMutation(),
            { queryClient: client },
        )

        const input = {
            customerId: 'c1',
            expectedLockVersion: 3,
            expectedPartyVersion: 2,
            baseRevisionId: 'r1',
            legalName: '示例有限公司',
            status: 'active' as const,
            changeReason: '更新名称',
            idempotencyKey: 'revise-abc',
        }
        let value: CustomerMutationResult | undefined
        await act(async () => {
            value = await result.current.mutateAsync(input)
        })

        expect(mockedApi.saveCustomerDetails).toHaveBeenCalledWith(input)
        expect(value).toEqual(succeededResult('C-1'))
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customers'],
        })
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.saveCustomerDetails.mockRejectedValue(new Error('save-fail'))

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useSaveCustomerDetailsMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current
                .mutateAsync({
                    customerId: 'c1',
                    expectedLockVersion: 3,
                    expectedPartyVersion: 2,
                    baseRevisionId: 'r1',
                    legalName: '示例有限公司',
                    status: 'active' as const,
                    changeReason: '更新名称',
                    idempotencyKey: 'revise-err',
                })
                .catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})

describe('useQueryCustomerIdempotencyMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('passes the idempotency key through to the api', async () => {
        mockedApi.queryCustomerMutationByIdempotency.mockResolvedValue(
            succeededResult('C-1'),
        )

        const { result } = renderHookWithProviders(() =>
            useQueryCustomerIdempotencyMutation(),
        )

        let value: CustomerMutationResult | null | undefined
        await act(async () => {
            value = await result.current.mutateAsync('create-abc')
        })

        expect(mockedApi.queryCustomerMutationByIdempotency).toHaveBeenCalledWith(
            'create-abc',
        )
        expect(value).toEqual(succeededResult('C-1'))
    })

    it('returns null when no recorded result exists', async () => {
        mockedApi.queryCustomerMutationByIdempotency.mockResolvedValue(null)

        const { result } = renderHookWithProviders(() =>
            useQueryCustomerIdempotencyMutation(),
        )

        await act(async () => {
            await expect(
                result.current.mutateAsync('missing'),
            ).resolves.toBeNull()
        })
    })
})

describe('useApplyCustomerAssignmentMutation', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('wires mutationFn to applyCustomerAssignment and invalidates customer queries on success', async () => {
        mockedApi.applyCustomerAssignment.mockResolvedValue([])

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useApplyCustomerAssignmentMutation(),
            { queryClient: client },
        )

        const input = {
            customerId: 'c1',
            action: 'assign' as const,
            userId: 'u1',
            role: 'COLLABORATOR' as const,
            effectiveFrom: '2026-01-01',
            changeReason: '补充协作',
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })

        expect(mockedApi.applyCustomerAssignment).toHaveBeenCalledWith(input)
        await waitFor(() => expect(invalidateSpy).toHaveBeenCalledTimes(1))
        expect(invalidateSpy).toHaveBeenCalledWith({
            queryKey: ['customers'],
        })
    })

    it('propagates mutation errors without invalidating', async () => {
        mockedApi.applyCustomerAssignment.mockRejectedValue(
            new Error('assign-fail'),
        )

        const client = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(client, 'invalidateQueries')
        const { result } = renderHookWithProviders(
            () => useApplyCustomerAssignmentMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current
                .mutateAsync({
                    customerId: 'c1',
                    action: 'end' as const,
                    assignmentId: 'as1',
                    version: 1,
                    effectiveTo: '2026-09-01',
                    changeReason: '结束协作',
                })
                .catch(() => undefined)
        })

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})
