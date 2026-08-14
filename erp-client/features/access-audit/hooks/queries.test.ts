import { beforeEach, describe, expect, it, vi } from 'vitest'
import { act, waitFor } from '@testing-library/react'

import {
    fetchAccessList,
    fetchAuditEvent,
    fetchEffectiveAccess,
    previewAccessChange,
    submitAccessChange,
} from '@/features/access-audit/api'
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from '@/features/test-utils'
import type {
    AccessChangeCommand,
    AccessChangeOutcome,
    AccessListQuery,
} from '../types'
import { makeAuditRow, makeGovernancePolicies, makeListView } from './test-data'
import {
    useAccessListQuery,
    useAuditEventQuery,
    useEffectiveAccessQuery,
    usePreviewAccessChangeMutation,
    useSubmitAccessChangeMutation,
} from './queries'

vi.mock('@/features/access-audit/api', () => ({
    fetchAccessList: vi.fn(),
    fetchAuditEvent: vi.fn(),
    fetchEffectiveAccess: vi.fn(),
    previewAccessChange: vi.fn(),
    submitAccessChange: vi.fn(),
}))

const mockedFetchAccessList = vi.mocked(fetchAccessList)
const mockedFetchAuditEvent = vi.mocked(fetchAuditEvent)
const mockedFetchEffectiveAccess = vi.mocked(fetchEffectiveAccess)
const mockedPreviewAccessChange = vi.mocked(previewAccessChange)
const mockedSubmitAccessChange = vi.mocked(submitAccessChange)

const listQuery: AccessListQuery = { view: 'roles' }

const revokeCommand: AccessChangeCommand = {
    subjectType: 'USER',
    subjectId: 'u1',
    action: 'EMERGENCY_REVOKE_USER_ROLE',
    roleAssignmentId: 'ura-1',
    expectedPermissionVersion: 'pv-live',
    reasonCode: 'EMERGENCY_STOP_LOSS',
    idempotencyKey: 'w19-test-key',
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe('useAccessListQuery', () => {
    it('fetches the list under the access-audit/list key and hands the query to the api', async () => {
        const fixture = makeListView()
        mockedFetchAccessList.mockResolvedValue(fixture)
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useAccessListQuery(listQuery),
            { queryClient },
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.data).toEqual(fixture))

        expect(mockedFetchAccessList).toHaveBeenCalledWith(listQuery)
        const keys = queryClient.getQueryCache().findAll().map((q) => q.queryKey)
        expect(keys).toEqual([['access-audit', 'list', listQuery]])
    })

    it('reuses the cached entry across rerenders with the same query object', async () => {
        mockedFetchAccessList.mockResolvedValue(makeListView())
        const queryClient = createFreshQueryClient()

        const { result, rerender } = renderHookWithProviders(
            () => useAccessListQuery(listQuery),
            { queryClient },
        )

        await waitFor(() => expect(result.current.data).toBeDefined())
        expect(mockedFetchAccessList).toHaveBeenCalledTimes(1)

        rerender()
        await waitFor(() => expect(result.current.isFetching).toBe(false))
        expect(mockedFetchAccessList).toHaveBeenCalledTimes(1)
    })

    it('surfaces errors from the api', async () => {
        mockedFetchAccessList.mockRejectedValue(new Error('network down'))
        const { result } = renderHookWithProviders(() =>
            useAccessListQuery(listQuery),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe('useEffectiveAccessQuery', () => {
    it('stays disabled and never fetches without a subject', () => {
        const { result } = renderHookWithProviders(() =>
            useEffectiveAccessQuery(null, null),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(result.current.data).toBeUndefined()
        expect(mockedFetchEffectiveAccess).not.toHaveBeenCalled()
    })

    it('fetches effective access for the given subject under a stable key', async () => {
        const fixture = {
            subject: { type: 'ROLE' as const, id: 'role-1', label: '管理员' },
            moduleAndActionGrants: [],
            dataScopes: [],
            fieldPolicies: [],
            historicalParticipantRules: [],
            deniedOrBlocked: [],
            permissionVersion: 'pv-live',
            calculatedAt: '2026-08-14T10:00:00.000Z',
            governancePolicies: makeGovernancePolicies(),
            allowedActions: ['VIEW_EFFECTIVE_ACCESS'],
            actionBlockers: [],
        }
        mockedFetchEffectiveAccess.mockResolvedValue(fixture)
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(
            () => useEffectiveAccessQuery('USER', 'u1'),
            { queryClient },
        )

        await waitFor(() => expect(result.current.data).toEqual(fixture))
        expect(mockedFetchEffectiveAccess).toHaveBeenCalledWith('USER', 'u1')
        expect(
            queryClient.getQueryCache().findAll().map((q) => q.queryKey),
        ).toEqual([['access-audit', 'effective', 'USER', 'u1']])
    })
})

describe('useAuditEventQuery', () => {
    it('stays disabled and never fetches without an event id', () => {
        const { result } = renderHookWithProviders(() =>
            useAuditEventQuery(null),
        )

        expect(result.current.fetchStatus).toBe('idle')
        expect(mockedFetchAuditEvent).not.toHaveBeenCalled()
    })

    it('fetches the event detail for the given id', async () => {
        const fixture = makeAuditRow()
        mockedFetchAuditEvent.mockResolvedValue(fixture)

        const { result } = renderHookWithProviders(() =>
            useAuditEventQuery('ae-1'),
        )

        await waitFor(() => expect(result.current.data).toEqual(fixture))
        expect(mockedFetchAuditEvent).toHaveBeenCalledWith('ae-1')
    })
})

describe('usePreviewAccessChangeMutation', () => {
    it('wires mutationFn to previewAccessChange', async () => {
        const preview = {
            subjectLabel: 'u1',
            actionLabel: 'EMERGENCY_REVOKE_USER_ROLE',
            changeSummary: '预览',
            affectedSubjectCount: 1,
            affectedWorkSurfaceSummary: '影响预览尚未可用',
            riskLevel: 'medium' as const,
            riskSummary: '风险摘要',
            riskFlags: [],
            diffs: [],
        }
        mockedPreviewAccessChange.mockResolvedValue(preview)

        const { result } = renderHookWithProviders(() =>
            usePreviewAccessChangeMutation(),
        )

        let pending: Promise<unknown>
        act(() => {
            pending = result.current.mutateAsync(revokeCommand)
        })
        await pending!

        expect(mockedPreviewAccessChange).toHaveBeenCalledWith(revokeCommand)
        await waitFor(() => expect(result.current.data).toEqual(preview))
    })
})

describe('useSubmitAccessChangeMutation', () => {
    const confirmedOutcome: AccessChangeOutcome = {
        outcome: 'CONFIRMED',
        permissionVersion: 'pv-live',
        auditEventId: 'ae_1',
        affectedSubjectCount: 1,
        effectiveAt: '2026-08-14T10:00:00.000Z',
        reference: 'w19-test-key',
        nextSteps: ['刷新用户授权列表'],
        message: '已提交紧急撤权。',
    }

    it('wires mutationFn to submitAccessChange and invalidates access-audit on CONFIRMED', async () => {
        mockedSubmitAccessChange.mockResolvedValue(confirmedOutcome)
        const queryClient = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')

        const { result } = renderHookWithProviders(
            () => useSubmitAccessChangeMutation(),
            { queryClient },
        )

        let pending: Promise<unknown>
        act(() => {
            pending = result.current.mutateAsync(revokeCommand)
        })
        await pending!

        expect(mockedSubmitAccessChange).toHaveBeenCalledWith(revokeCommand)
        await waitFor(() =>
            expect(invalidateSpy).toHaveBeenCalledWith({
                queryKey: ['access-audit'],
            }),
        )
        expect(result.current.data).toEqual(confirmedOutcome)
    })

    it('does not invalidate the cache when the outcome is REJECTED', async () => {
        mockedSubmitAccessChange.mockResolvedValue({
            outcome: 'REJECTED',
            code: 'UNSUPPORTED_COMMAND',
            message: '未映射到后端写路径',
        })
        const queryClient = createFreshQueryClient()
        const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')

        const { result } = renderHookWithProviders(
            () => useSubmitAccessChangeMutation(),
            { queryClient },
        )

        let pending: Promise<unknown>
        act(() => {
            pending = result.current.mutateAsync(revokeCommand)
        })
        await pending!

        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(invalidateSpy).not.toHaveBeenCalled()
    })
})
