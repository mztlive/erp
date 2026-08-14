import { act, renderHook, waitFor } from "@testing-library/react"
import { QueryClientProvider } from "@tanstack/react-query"
import type { ReactNode } from "react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import * as api from "./api"
import {
    synchronizeWorkItemConflict,
    useBlockedApprovalsQuery,
    useRecoverApprovalMutation,
    useWorkItemDetailQuery,
    useWorkItemResponsibilityMutation,
    useWorkItemsQuery,
    useWorkItemStatsQuery,
    workItemKeys,
} from "./queries"
import type {
    BlockedApprovalView,
    WorkItemConflict,
    WorkItemDto,
    WorkItemResponsibilityCommand,
} from "./types"
import type { WorkItemPage, WorkItemStats } from "./api"

vi.mock("./api", () => ({
    getWorkItem: vi.fn(),
    getWorkItemStats: vi.fn(),
    listBlockedApprovals: vi.fn(),
    listWorkItems: vi.fn(),
    parseWorkItemConflict: vi.fn(),
    recoverApproval: vi.fn(),
    submitWorkItemResponsibility: vi.fn(),
}))

const makeWorkItemDto = (overrides: Partial<WorkItemDto> = {}): WorkItemDto => ({
    id: "wi_1",
    work_item_type: "procurement_confirmation",
    handler_key: "procurement-confirmation",
    approval_step_instance_id: null,
    status: "OPEN",
    assignment_mode: "DIRECT",
    assignment_source: "assigned",
    owner_role: "procurement",
    owner_organization_id: "org_1",
    processing_state: "READY",
    business_object_type: "purchase_plan",
    business_object_id: "po_1",
    root_business_object_id: "po_1",
    subject_version: "v3",
    task_version: "5",
    priority: "HIGH",
    created_at: 1_700_000_000_000,
    ...overrides,
})

const listParams = {
    scope: "mine" as const,
    timezone: "Asia/Shanghai",
}

const makePage = (): WorkItemPage => ({
    items: [makeWorkItemDto()],
    total: 1,
    page: 1,
    page_size: 100,
})

const makeStats = (): WorkItemStats => ({
    assigned: 2,
    team: 1,
    due_today: 1,
    overdue: 0,
    exception: 0,
    as_of: 1_700_000_000_000,
})

const makeBlockedApproval = (): BlockedApprovalView => ({
    approvalInstanceId: "ai_1",
    instanceVersion: "2",
    currentStepInstanceId: "si_1",
    stepVersion: "1",
    businessObjectLabel: "采购计划 PO-1",
    blockerCode: "STEP_BLOCKED",
    blockerMessage: "请处理后继续",
    blockedAt: 1_700_000_000_000,
    allowedActions: ["RETRY_CURRENT_STEP"],
})

const conflict = (currentWorkItem: WorkItemDto | null): WorkItemConflict => ({
    code: "WORK_ITEM_RESPONSIBILITY_CONFLICT",
    currentWorkItem,
})

const makeConflictError = (
    currentWorkItem: WorkItemDto | null,
    status = 409,
): { status: number; responseData: unknown } => ({
    status,
    responseData: { data: { current_work_item: currentWorkItem } },
})

const startCommand: WorkItemResponsibilityCommand = {
    kind: "START_PROCESSING",
    workItemId: "wi_1",
    expectedTaskVersion: "5",
    idempotencyKey: "op_1",
}

beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(api.parseWorkItemConflict).mockReturnValue(undefined)
})

describe("useWorkItemsQuery", () => {
    it("fetches the list under a stable key and reuses it across rerenders", async () => {
        vi.mocked(api.listWorkItems).mockResolvedValue(makePage())
        const client = createFreshQueryClient()
        const { result, rerender } = renderHookWithProviders(
            () => useWorkItemsQuery(listParams),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.items).toHaveLength(1)
        expect(api.listWorkItems).toHaveBeenCalledTimes(1)
        expect(api.listWorkItems).toHaveBeenCalledWith(listParams)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ["work-items", "list", listParams],
        ])

        rerender()
        await waitFor(() => expect(client.isFetching()).toBe(0))
        expect(api.listWorkItems).toHaveBeenCalledTimes(1)
    })

    it("exposes the error state when the request fails", async () => {
        vi.mocked(api.listWorkItems).mockRejectedValue(new Error("list failed"))
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useWorkItemsQuery(listParams),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
        expect(result.current.data).toBeUndefined()
    })
})

describe("useWorkItemDetailQuery", () => {
    const wrapper =
        (client: ReturnType<typeof createFreshQueryClient>) =>
        ({ children }: { children: ReactNode }) => (
            <QueryClientProvider client={client}>{children}</QueryClientProvider>
        )

    it("stays disabled for blank ids and fetches for non-blank ids", async () => {
        vi.mocked(api.getWorkItem).mockResolvedValue(makeWorkItemDto())
        const client = createFreshQueryClient()

        const { result, rerender } = renderHook(
            ({ workItemId }: { workItemId: string }) =>
                useWorkItemDetailQuery(workItemId),
            {
                wrapper: wrapper(client),
                initialProps: { workItemId: "   " },
            },
        )
        expect(result.current.isPending).toBe(true)
        expect(result.current.fetchStatus).toBe("idle")
        expect(api.getWorkItem).not.toHaveBeenCalled()

        rerender({ workItemId: "wi_1" })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.id).toBe("wi_1")
        expect(api.getWorkItem).toHaveBeenCalledWith("wi_1")
        expect(
            client
                .getQueryCache()
                .getAll()
                .some(
                    (q) =>
                        JSON.stringify(q.queryKey) ===
                        JSON.stringify(["work-items", "detail", "wi_1"]),
                ),
        ).toBe(true)
    })

    it("resolves null as a valid not-found result", async () => {
        vi.mocked(api.getWorkItem).mockResolvedValue(
            null as unknown as WorkItemDto,
        )
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useWorkItemDetailQuery("wi_missing"),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBeNull()
    })
})

describe("useWorkItemStatsQuery", () => {
    it("fetches stats with the given params under a stable key", async () => {
        vi.mocked(api.getWorkItemStats).mockResolvedValue(makeStats())
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useWorkItemStatsQuery(listParams),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.assigned).toBe(2)
        expect(api.getWorkItemStats).toHaveBeenCalledWith(listParams)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ["work-items", "stats", listParams],
        ])
    })
})

describe("useBlockedApprovalsQuery", () => {
    it("fetches the blocked approval list when enabled", async () => {
        vi.mocked(api.listBlockedApprovals).mockResolvedValue({
            items: [makeBlockedApproval()],
            total: 1,
            page: 1,
            page_size: 100,
        })
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useBlockedApprovalsQuery(),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.items[0]?.blockerCode).toBe("STEP_BLOCKED")
        expect(api.listBlockedApprovals).toHaveBeenCalledTimes(1)
        expect(client.getQueryCache().getAll().map((q) => q.queryKey)).toEqual([
            ["approval-instances", "blocked"],
        ])
    })

    it("does not fetch when disabled", () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useBlockedApprovalsQuery(false),
            { queryClient: client },
        )
        expect(result.current.fetchStatus).toBe("idle")
        expect(api.listBlockedApprovals).not.toHaveBeenCalled()
    })
})

describe("useWorkItemResponsibilityMutation", () => {
    it("wires submitWorkItemResponsibility and invalidates work item queries on success", async () => {
        vi.mocked(api.submitWorkItemResponsibility).mockResolvedValue(
            makeWorkItemDto(),
        )
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useWorkItemResponsibilityMutation(),
            { queryClient: client },
        )

        let outcome: unknown
        await act(async () => {
            outcome = await result.current.mutateAsync(startCommand)
        })
        expect(outcome).toEqual(makeWorkItemDto())
        expect(api.submitWorkItemResponsibility).toHaveBeenCalledWith(
            startCommand,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["work-items"],
            }),
        )
    })

    it("seeds the detail cache with the conflict summary and invalidates on 409", async () => {
        const stale = makeWorkItemDto({ task_version: "3" })
        const current = makeWorkItemDto({ task_version: "7" })
        const failure = makeConflictError(current)
        vi.mocked(api.submitWorkItemResponsibility).mockRejectedValue(failure)
        vi.mocked(api.parseWorkItemConflict).mockReturnValue(
            conflict(current),
        )
        const client = createFreshQueryClient()
        client.setQueryData(workItemKeys.detail("wi_1"), stale)
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useWorkItemResponsibilityMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await expect(result.current.mutateAsync(startCommand)).rejects.toBe(
                failure,
            )
        })
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["work-items"],
            }),
        )
        expect(client.getQueryData(workItemKeys.detail("wi_1"))).toEqual(
            current,
        )
    })

    it("nulls the detail cache when the conflict summary hides the task", async () => {
        const stale = makeWorkItemDto()
        const failure = makeConflictError(null)
        vi.mocked(api.submitWorkItemResponsibility).mockRejectedValue(failure)
        vi.mocked(api.parseWorkItemConflict).mockReturnValue(conflict(null))
        const client = createFreshQueryClient()
        client.setQueryData(workItemKeys.detail("wi_1"), stale)
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useWorkItemResponsibilityMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await expect(result.current.mutateAsync(startCommand)).rejects.toBe(
                failure,
            )
        })
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["work-items"],
            }),
        )
        expect(client.getQueryData(workItemKeys.detail("wi_1"))).toBeNull()
    })

    it("invalidates on a non-conflict 409 without touching the detail cache", async () => {
        const stale = makeWorkItemDto()
        const failure = makeConflictError(null)
        vi.mocked(api.submitWorkItemResponsibility).mockRejectedValue(failure)
        vi.mocked(api.parseWorkItemConflict).mockReturnValue(undefined)
        const client = createFreshQueryClient()
        client.setQueryData(workItemKeys.detail("wi_1"), stale)
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useWorkItemResponsibilityMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await expect(result.current.mutateAsync(startCommand)).rejects.toBe(
                failure,
            )
        })
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["work-items"],
            }),
        )
        expect(client.getQueryData(workItemKeys.detail("wi_1"))).toEqual(stale)
    })
})

describe("useRecoverApprovalMutation", () => {
    const recoverCommand = {
        approvalInstanceId: "ai_1",
        currentStepInstanceId: "si_1",
        expectedInstanceVersion: "2",
        expectedStepVersion: "1",
        recoveryAction: "RETRY_CURRENT_STEP" as const,
        reason: "retry",
        idempotencyKey: "recover_1",
    }

    it("wires recoverApproval and invalidates blockers plus work item queries on success", async () => {
        vi.mocked(api.recoverApproval).mockResolvedValue({
            instance: {
                id: "ai_1",
                definition_key: "purchase_plan",
                definition_version: 1,
                runtime_kind: "INTERNAL",
                business_object_type: "purchase_plan",
                business_object_id: "po_1",
                subject_version: "v1",
                owner_organization_id: "org_1",
                status: "RUNNING",
                instance_version: "3",
                started_by: "u1",
                started_at: 1_700_000_000_000,
            },
            step: {
                id: "si_1",
                approval_instance_id: "ai_1",
                step_key: "confirm",
                sequence_no: 1,
                status: "ACTIVE",
                step_version: "2",
            },
        })
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useRecoverApprovalMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await result.current.mutateAsync(recoverCommand)
        })
        expect(api.recoverApproval).toHaveBeenCalledWith(
            recoverCommand,
            expect.anything(),
        )
        await waitFor(() => {
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["approval-instances", "blocked"],
            })
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["work-items"],
            })
        })
    })

    it("does not invalidate when the recovery request fails", async () => {
        vi.mocked(api.recoverApproval).mockRejectedValue(new Error("boom"))
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useRecoverApprovalMutation(),
            { queryClient: client },
        )

        await act(async () => {
            await expect(result.current.mutateAsync(recoverCommand)).rejects.toBeInstanceOf(
                Error,
            )
        })
        expect(invalidate).not.toHaveBeenCalled()
    })
})

describe("synchronizeWorkItemConflict", () => {
    it("invalidates work item queries for a plain 409 without a parsed conflict", async () => {
        vi.mocked(api.parseWorkItemConflict).mockReturnValue(undefined)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")

        await synchronizeWorkItemConflict(client, startCommand, {
            status: 409,
        })

        expect(invalidate).toHaveBeenCalledWith({ queryKey: ["work-items"] })
    })

    it("does nothing for a non-409 error without a parsed conflict", async () => {
        vi.mocked(api.parseWorkItemConflict).mockReturnValue(undefined)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")

        await synchronizeWorkItemConflict(client, startCommand, {
            status: 500,
        })

        expect(invalidate).not.toHaveBeenCalled()
    })

    it("keeps the detail cache unchanged when the conflict summary is another task", async () => {
        const other = makeWorkItemDto({ id: "wi_2", task_version: "9" })
        vi.mocked(api.parseWorkItemConflict).mockReturnValue(conflict(other))
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")

        await synchronizeWorkItemConflict(client, startCommand, {
            status: 409,
        })

        expect(client.getQueryData(workItemKeys.detail("wi_1"))).toBeNull()
        expect(invalidate).toHaveBeenCalledWith({ queryKey: ["work-items"] })
    })
})
